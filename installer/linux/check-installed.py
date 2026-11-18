"""Exercise an installed launcher in a disposable Linux account on ext4."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import sqlite3
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
import uuid


def run(args, **kwargs):
    return subprocess.run(list(map(str, args)), capture_output=True, timeout=120, **kwargs)


def wait(check):
    end = time.monotonic() + 65
    while time.monotonic() < end:
        if check():
            return
        time.sleep(.5)
    raise RuntimeError("Timed out waiting for installed behavior")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--disposable-account", required=True, help="Exact name of a dedicated test account")
    parser.add_argument("--launcher", type=Path, required=True)
    parser.add_argument("--app", type=Path, required=True)
    args = parser.parse_args()
    import pwd
    assert pwd.getpwuid(os.getuid()).pw_name == args.disposable_account
    home = Path.home()
    assert home == Path(pwd.getpwuid(os.getuid()).pw_dir)
    state = home / ".local/state/coven-host-desktop"
    root = home / "Coven"
    app = args.app
    cli = [app / "coven", "host"]
    launcher = [args.launcher]
    run(launcher, check=True)
    metadata = json.loads((state / "desktop.json").read_text())
    url = "http://127.0.0.1:" + str(metadata["port"])
    token = state / "access-token"
    assert token.stat().st_mode & 0o777 == 0o600
    assert state.stat().st_mode & 0o777 == 0o700
    with sqlite3.connect((state / "client/client.sqlite").as_uri() + "?mode=ro", uri=True) as db:
        identity = db.execute("SELECT value FROM settings WHERE key='host'").fetchone()[0]
    saved = hashlib.sha256(token.read_bytes()).digest()
    try:
        urllib.request.urlopen(url + "/v1/castles/demo/files", timeout=5)
    except urllib.error.HTTPError as err:
        assert err.code == 401
    else:
        raise AssertionError("The peer accepted an unauthenticated request")
    opts = ["--url", url, "--token-file", token]
    key = uuid.uuid4().hex
    note = "files/check-" + key + ".txt"
    result = "results/check-" + key + ".txt"
    files = root / "Castles/demo"
    (files / note).write_bytes(b"locally edited\n")
    wait(lambda: run([*cli, "get", note, *opts]).stdout == b"locally edited\n")
    with tempfile.NamedTemporaryFile() as src:
        src.write(b"result from peer\n")
        src.flush()
        run([*cli, "put", result, src.name, *opts], check=True)
    wait(lambda: (files / result).is_file() and (files / result).read_bytes() == b"result from peer\n")
    backend = (state / "startup").read_text()
    def stop_service():
        if backend == "systemd":
            unit = "coven-host-desktop.service"
            run(["systemctl", "--user", "is-enabled", unit], check=True)
            run(["systemctl", "--user", "stop", unit], check=True)
        else:
            pid = int((state / "session-ready").read_text())
            os.kill(pid, signal.SIGTERM)
            wait(lambda: not (state / "session-ready").exists())
            assert (home / ".config/autostart/org.opencoven.CovenHost.desktop").is_file()

    stop_service()
    (files / note).write_bytes(b"saved offline\n")
    env = dict(os.environ, COVEN_HOME=str(state / "profile"), COVEN_HOST_STATE=str(state / "client"))
    offline = run([*cli, "sync"], env=env)
    assert offline.returncode and json.loads(offline.stdout)["phase"] == "offline"
    run(launcher, check=True)
    wait(lambda: run([*cli, "get", note, *opts]).stdout == b"saved offline\n")
    assert metadata == json.loads((state / "desktop.json").read_text())
    assert saved == hashlib.sha256(token.read_bytes()).digest()
    with sqlite3.connect((state / "client/client.sqlite").as_uri() + "?mode=ro", uri=True) as db:
        assert identity == db.execute("SELECT value FROM settings WHERE key='host'").fetchone()[0]
    assert b"CH-LINUX-OK" in run([*launcher, "--diagnostics"], check=True).stdout
    stop_service()
    (files / note).write_bytes(b"my conflict version\n")
    with tempfile.NamedTemporaryFile() as src:
        src.write(b"peer conflict version\n")
        src.flush()
        peer = subprocess.Popen([*cli, "serve", "--port", str(metadata["port"]), "--storage", state / "server",
                                 "--token-file", token], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            wait(lambda: run([*cli, "get", note, *opts]).returncode == 0)
            run([*cli, "put", note, src.name, *opts], check=True)
        finally:
            peer.terminate()
            peer.wait(timeout=10)
    conflicted = run([*launcher, "--diagnostics"])
    assert conflicted.returncode and b"CH-LINUX-CONFLICT" in conflicted.stderr
    assert str(home).encode() not in conflicted.stderr and token.read_bytes().strip() not in conflicted.stderr
    run([*cli, "resolve", note, "--local"], env=env, check=True)
    wait(lambda: run([*cli, "get", note, *opts]).stdout == b"my conflict version\n")
    print(f"PASS {backend} • private state and token • unauthorized access rejected • two-way automatic sync • offline save and retry • stable peer, folder and token • diagnostic report • two-writer conflict diagnosis and recovery")


if __name__ == "__main__":
    main()
