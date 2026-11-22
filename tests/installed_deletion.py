"""Exercise an installed Coven binary using only temporary peer storage and profiles."""
import csv
import hashlib
import http.client
import json
import os
from pathlib import Path
import secrets
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time


def until(probe):
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        if probe():
            return
        time.sleep(0.25)
    raise RuntimeError("Timed out waiting for installed sync")


def stop(process):
    if process is not None and process.poll() is None:
        process.terminate()
        process.wait(timeout=15)


def exercise(binary, work):
    env = dict(os.environ, COVEN_HOME=str(work / "profile"))
    env.pop("COVEN_HOST_STATE", None)
    token = work / "access"
    token.write_text(secrets.token_hex(32), encoding="ascii")
    if os.name == "nt":
        sid = next(csv.reader(subprocess.check_output(
            ["whoami", "/user", "/fo", "csv", "/nh"], text=True).splitlines()))[1]
        subprocess.run(["icacls", token, "/inheritance:r", "/grant:r", f"*{sid}:(F)"],
                       stdout=subprocess.DEVNULL, check=True)
    else:
        token.chmod(0o600)
    root = work / "castle"
    storage = work / "peer"
    files = root / "Castles/demo/files"
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        port = s.getsockname()[1]
    url = f"http://127.0.0.1:{port}"
    auth = ["--url", url, "--token-file", token]

    def cli(*args, success=True):
        p = subprocess.run([binary, *args], env=env, capture_output=True, timeout=60)
        if (p.returncode == 0) != success:
            raise RuntimeError(f"{args[:2]} returned {p.returncode}: {p.stdout[-400:]} {p.stderr[-400:]}")
        return p.stdout

    def peer_start():
        return subprocess.Popen([binary, "host", "serve", "--storage", storage, "--port",
                                 str(port), "--token-file", token], env=env, stdout=subprocess.DEVNULL)

    def request(method, suffix="", **headers):
        conn = http.client.HTTPConnection("127.0.0.1", port, timeout=8)
        try:
            conn.request(method, "/v1/castles/demo/files" + suffix,
                         headers={"Authorization": "Bearer " + token.read_text(),
                                  "X-Coven-Deletions": "1", **headers})
            response = conn.getresponse()
            return response.status, response.read()
        finally:
            conn.close()

    def inventory():
        code, raw = request("GET")
        assert code == 200
        return json.loads(raw)

    def ready():
        try:
            return "deleted" in inventory()
        except OSError:
            return False

    def status():
        return json.loads(cli("host", "status"))

    def archive_bytes(name, data):
        with sqlite3.connect(storage / "host.sqlite") as db:
            rows = db.execute("SELECT bytes,sha256 FROM deleted_versions WHERE path=?", (name,))
            assert (data, hashlib.sha256(data).hexdigest()) in rows.fetchall()

    peer = watcher = None
    try:
        peer = peer_start()
        until(ready)
        cli("host", "init", "--root", root)
        cli("host", "connect", url, "--token-file", token)
        before = files / "before.txt"
        after = files / "after.txt"
        content = b"offline rename\x00\r\n"
        before.write_bytes(content)
        watcher = subprocess.Popen([binary, "host", "watch"], env=env, stdout=subprocess.DEVNULL)
        until(lambda: request("GET", "/files/before.txt") == (200, content))
        stop(watcher)
        stop(peer)
        before.rename(after)
        cli("daemon", "restart")
        peer = peer_start()
        until(ready)
        watcher = subprocess.Popen([binary, "host", "watch"], env=env, stdout=subprocess.DEVNULL)
        until(lambda: inventory()["deleted"].get("files/before.txt") == 2 and
              request("GET", "/files/after.txt") == (200, content))
        assert request("GET", "/files/before.txt")[0] == 404
        assert any(row["path"] == "files/after.txt" and row["revision"] == 1 for row in inventory()["files"])
        archive_bytes("files/before.txt", content)
        stop(watcher)
        cli("daemon", "restart")
        print("PASS offline rename, daemon and peer restart, automatic watcher convergence and peer recovery", flush=True)

        path = files / "conflict.txt"
        path.write_bytes(b"base")
        cli("host", "sync")
        stop(peer)
        path.unlink()
        cli("daemon", "restart")
        peer = peer_start()
        until(ready)
        edit = b"peer edit\x00\r\n"
        source = work / "edit"
        source.write_bytes(edit)
        cli("host", "put", "files/conflict.txt", source, *auth)
        cli("host", "sync", success=False)
        conflict, = status()["conflicts"]
        assert conflict["localDeleted"] and conflict["revision"] == 2
        copy = Path(conflict["remote"])
        assert copy.read_bytes() == edit
        cli("host", "resolve", "files/conflict.txt", "--local")
        cli("host", "sync")
        assert inventory()["deleted"]["files/conflict.txt"] == 3
        assert request("GET", "/files/conflict.txt")[0] == 404 and not path.exists()
        assert copy.read_bytes() == edit
        archive_bytes("files/conflict.txt", edit)
        print("PASS delete versus edit, status recovery copy, explicit resolution and retained exact peer bytes", flush=True)

        # Both sides can finish deleting a conflicted file while the daemon is offline.
        path.write_bytes(b"another base")
        cli("host", "sync")
        path.write_bytes(edit)
        active = next(row for row in inventory()["files"] if row["path"] == "files/conflict.txt")
        code, _ = request("DELETE", "/files/conflict.txt", **{
            "X-Host-ID": inventory()["host"], "If-Match": f'"{active["revision"]}"',
            "X-Sync-Operation": secrets.token_hex(16)})
        assert code == 200
        cli("host", "sync", success=False)
        conflict, = status()["conflicts"]
        archived = Path(conflict["local"])
        assert archived.read_bytes() == edit
        path.unlink()
        cli("daemon", "restart")
        cli("host", "sync")
        assert not status()["conflicts"] and archived.read_bytes() == edit
        assert request("GET", "/files/conflict.txt")[0] == 404 and not path.exists()
        print("PASS both sides deleted after conflict; archive retained through daemon restart", flush=True)

        if os.name == "nt":
            held = files / "held.txt"
            held.write_bytes(content)
            cli("host", "sync")
            with held.open("rb") as opened:
                code, _ = request("DELETE", "/files/held.txt", **{
                    "X-Host-ID": inventory()["host"], "If-Match": '"1"',
                    "X-Sync-Operation": secrets.token_hex(16)})
                assert code == 200
                cli("host", "sync", success=False)
                assert opened.read() == held.read_bytes() == content
            cli("host", "sync")
            current = status()
            assert not held.exists() and current["phase"] == "synced" and not current["conflicts"]
            assert any(Path(p).read_bytes() == content for p in current["previousVersions"]
                       if "held.txt" in Path(p).parts)
            archive_bytes("files/held.txt", content)
            print("PASS NTFS sharing violation preserved the held file, then recovered after handle close", flush=True)
    finally:
        stop(watcher)
        subprocess.run([binary, "daemon", "stop"], env=env, capture_output=True, timeout=30)
        stop(peer)


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="castle-delete-") as fixture:
        exercise(Path(sys.argv[1]).resolve(strict=True), Path(fixture).resolve())
