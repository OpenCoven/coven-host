"""Run against an installed Coven CLI using a disposable profile and peer."""
import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import urllib.request

cli = Path(sys.argv[1]).absolute()


def qualify(work):
    home = work / "profile"
    root = work / "root"
    files = root / "Castles/demo/files"
    token = work / "access"
    token.write_text(secrets.token_urlsafe(32))
    token.chmod(0o600)
    env = dict(os.environ, COVEN_HOME=str(home))
    env.pop("COVEN_HOST_STATE", None)
    peer = None
    port = 0

    def run(*args, ok=True):
        p = subprocess.run([cli, *map(str, args)], env=env, capture_output=True, timeout=120)
        if (p.returncode == 0) != ok:
            raise RuntimeError((args[:2], p.returncode, p.stdout, p.stderr))
        return p.stdout

    def host(*args, ok=True):
        return run("host", *args, ok=ok)

    def start():
        nonlocal peer, port
        peer = subprocess.Popen([cli, "host", "serve", "--port", str(port), "--storage",
                                 work / "peer", "--token-file", token], env=env, stdout=subprocess.PIPE)
        line = peer.stdout.readline().decode()
        assert line.startswith("Serving Castle demo on "), line
        port = int(line.strip().rsplit(":", 1)[1])

    def stop():
        nonlocal peer
        if peer:
            peer.terminate()
            peer.wait(timeout=30)
            peer = None

    def request(method="GET", path="", headers=None):
        req = urllib.request.Request(url + "/v1/castles/demo/files" + path, method=method,
            headers={"Authorization": "Bearer " + token.read_text(), "X-Coven-Deletions": "1", **(headers or {})})
        with urllib.request.urlopen(req, timeout=10) as resp:
            return resp.read()

    def inventory():
        return json.loads(request())

    def remove(name):
        inv = inventory()
        item = next(x for x in inv["files"] if x["path"] == "files/" + name)
        request("DELETE", "/files/" + name, {"X-Host-ID": inv["host"],
                "If-Match": '"%d"' % item["revision"], "X-Sync-Operation": secrets.token_hex(16)})

    def put(name, data):
        src = work / "upload"
        src.write_bytes(data)
        host("put", "files/" + name, src, "--url", url, "--token-file", token)

    def sync():
        assert json.loads(host("sync"))["phase"] == "synced"

    def verify(name, data, revision):
        inv = inventory()
        key = "files/" + name
        if data is None:
            assert inv["deleted"][key] == revision and not (files / name).exists()
            assert key not in [x["path"] for x in inv["files"]]
        else:
            item = next(x for x in inv["files"] if x["path"] == key)
            assert item["revision"] == revision and item["sha256"] == hashlib.sha256(data).hexdigest()
            assert (files / name).read_bytes() == request("GET", "/" + key) == data
        before = inventory()
        sync()
        assert inventory() == before

    def recover(path, expected):
        output = work / "recovered"
        shutil.copyfile(path, output)
        assert output.read_bytes() == expected
        print("Recovered SHA-256", hashlib.sha256(expected).hexdigest(), flush=True)

    try:
        start()
        url = "http://127.0.0.1:" + str(port)
        host("init", "--root", root)
        host("connect", url, "--token-file", token)
        run("daemon", "stop")
        database = home / "host/client.sqlite"
        selector = "SELECT name FROM sqlite_schema WHERE name IN ('pending_delete','deleted_baseline')"
        with sqlite3.connect(database) as db:
            db.executescript("DROP TABLE pending_delete; DROP TABLE deleted_baseline;")
        other = dict(env, COVEN_HOME=str(work / "competitor"), COVEN_HOST_STATE=str(database.parent))
        try:
            refused = subprocess.run([cli, "host", "status"], env=other, capture_output=True, timeout=120)
            assert refused.returncode and b"different daemon profile" in refused.stderr, refused.stderr
            with sqlite3.connect(database) as db:
                assert db.execute(selector).fetchall() == []
        finally:
            subprocess.run([cli, "daemon", "stop"], env=other, capture_output=True, timeout=60)
        sync()
        with sqlite3.connect(database) as db:
            assert len(db.execute(selector).fetchall()) == 2
        print("PASS wrong profile refused, owner migrated and synced", flush=True)
        data = b"offline rename\x00and recovery\xff"
        (files / "first").write_bytes(data)
        sync()
        run("daemon", "stop")
        stop()
        (files / "first").rename(files / "second")
        start()
        sync()
        verify("first", None, 2)
        verify("second", data, 1)
        run("daemon", "stop")
        stop()
        (files / "second").unlink()
        start()
        sync()
        verify("second", None, 2)
        with sqlite3.connect("file:" + str(work / "peer/host.sqlite") + "?mode=ro", uri=True) as db:
            archived = db.execute("SELECT bytes FROM deleted_versions WHERE path=? AND revision=2", ("files/second",)).fetchone()
            assert archived and archived[0] == data
        print("PASS offline client rename and deletion, peer archive", flush=True)

        put("remote", data)
        sync()
        run("daemon", "stop")
        put("moved", data)
        remove("remote")
        stop()
        start()
        sync()
        verify("remote", None, 2)
        verify("moved", data, 1)
        run("daemon", "stop")
        remove("moved")
        stop()
        start()
        sync()
        verify("moved", None, 2)
        previous = [Path(p) for p in json.loads(host("status"))["previousVersions"]]
        for name in ("remote", "moved"):
            recover(next(p for p in previous if p.parent.name == name), data)
        print("PASS offline peer rename and deletion, local previous versions", flush=True)

        for deleted in ("local", "remote"):
            for choice in ("local", "remote"):
                name = deleted + "-" + choice
                edit = ("edit-" + name).encode()
                put(name, b"baseline")
                sync()
                run("daemon", "stop")
                if deleted == "local":
                    (files / name).unlink()
                    put(name, edit)
                else:
                    (files / name).write_bytes(edit)
                    remove(name)
                stop()
                start()
                assert json.loads(host("sync", ok=False))["phase"] == "needs attention"
                conflict = next(x for x in json.loads(host("status"))["conflicts"] if x["path"] == "files/" + name)
                copy = Path(conflict["remote" if deleted == "local" else "local"])
                host("resolve", "files/" + name, "--" + choice)
                sync()
                revision = 2 if (deleted == "remote") == (choice == deleted) else 3
                verify(name, None if choice == deleted else edit, revision)
                recover(copy, edit)
                print("PASS", name, "revision", revision, flush=True)
    finally:
        try:
            run("daemon", "stop")
        finally:
            stop()


with tempfile.TemporaryDirectory(prefix="host5-") as temp:
    qualify(Path(temp).resolve())
