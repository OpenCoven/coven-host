"""Verify two independent Castle clients against one peer using an installed CLI or the Rust test adapter."""
import argparse
from contextlib import closing, ExitStack
from hashlib import sha256
from http.server import HTTPServer
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import threading
import unittest

from fault_proxy import FaultProxy
from installed_deletion import stop, until, write_token


KEY = "files/note.md"
ORIGINAL = b"initial document\x00\r\n"
EDIT = b"offline edit\x00\xff\n"


class AcknowledgementProbe(FaultProxy):
    def record_write(self):
        self.server.writes.append((self.command, self.headers.get("X-Sync-Operation")))
        self.relay()

    do_PUT = do_DELETE = record_write


class Client:
    def __init__(self, executable, home, adapter):
        self.state = home / "state"
        self.root = home / "Castle"
        self.file = self.root / "Castles" / "demo" / KEY
        self.env = dict(os.environ, COVEN_HOME=str(home / "profile"), COVEN_HOST_STATE=str(self.state))
        self.executable = executable
        self.adapter = adapter

    def run(self, *args, success=True):
        prefix = ["--state", self.state] if self.adapter else ["host"]
        p = subprocess.run([self.executable, *prefix, *args], env=self.env, capture_output=True, timeout=60)
        if (p.returncode == 0) != success:
            raise RuntimeError(f"{args[0]} exited {p.returncode}: {p.stderr.decode(errors='replace')[:300]}")
        return p.stdout

    def disconnect(self):
        if not self.adapter:
            subprocess.run([self.executable, "daemon", "stop"], env=self.env, capture_output=True, check=True, timeout=45)

    def status(self):
        output = self.run("status")
        return json.loads(output.splitlines()[-1] if self.adapter else output)


def rows(database, sql, *args):
    with closing(sqlite3.connect(database.as_uri() + "?mode=ro", uri=True)) as connection:
        return connection.execute(sql, args).fetchall()


def scenario(binary, work, adapter, first, keep):
    token = work / "access"
    write_token(token)
    clients = {name: Client(binary, work / name, adapter) for name in ("edit", "delete")}
    winner = clients[first]
    second = "edit" if first == "delete" else "delete"
    follower = clients[second]
    storage = work / "peer"
    peer_db = storage / "host.sqlite"
    port = 0
    peer = None

    def start_peer():
        nonlocal port, peer
        command = [binary, *([] if adapter else ["host"]), "serve", "--storage", storage,
                   "--port", str(port), "--token-file", token]
        log = work / "peer.log"
        with log.open("w") as output:
            peer = subprocess.Popen(command, env=winner.env, stdout=output)
        until(lambda: peer.poll() is not None or "Serving Castle demo on " in log.read_text())
        if peer.poll() is not None:
            raise RuntimeError("Disposable peer exited before accepting connections")
        port = int(log.read_text().splitlines()[0].rsplit(":", 1)[1])

    with ExitStack() as cleanup:
        cleanup.callback(lambda: stop(peer))
        start_peer()
        proxy = cleanup.enter_context(HTTPServer(("127.0.0.1", 0), AcknowledgementProbe))
        proxy.upstream, proxy.fault, proxy.writes = port, None, []
        thread = threading.Thread(target=proxy.serve_forever, daemon=True)
        thread.start()
        cleanup.callback(thread.join)
        cleanup.callback(proxy.shutdown)
        for client in clients.values():
            client.run("init", "--root", client.root)
            cleanup.callback(client.disconnect)
            client.run("connect", f"http://127.0.0.1:{proxy.server_port}", "--token-file", token)
        clients["edit"].file.write_bytes(ORIGINAL)
        for client in clients.values():
            client.run("sync")
            assert client.file.read_bytes() == ORIGINAL
            assert rows(client.state / "client.sqlite", "SELECT revision,sha FROM baseline WHERE path=?", KEY) == [(1, sha256(ORIGINAL).hexdigest())]
            client.disconnect()
        stop(peer)
        clients["edit"].file.write_bytes(EDIT)
        clients["delete"].file.unlink()
        start_peer()

        # Lose the response after the first client's mutation commits; restart both ends before retry.
        proxy.fault = "ack" if first == "edit" else "delete-ack"
        proxy.writes.clear()
        winner.run("sync", success=False)
        assert proxy.fault is None and len(proxy.writes) == 1
        operation = proxy.writes[0]
        assert operation[1]
        pending_table = "pending" if first == "edit" else "pending_delete"
        assert rows(winner.state / "client.sqlite", f"SELECT operation FROM {pending_table} WHERE path=?", KEY) == [(operation[1],)]
        winner.disconnect()
        stop(peer)
        start_peer()
        follower.run("sync", success=False)
        # The other client observes the committed change while its acknowledgement is still pending.
        conflict, = follower.status()["conflicts"]
        assert conflict["path"] == KEY and conflict["revision"] == 2
        assert conflict["remoteDeleted" if first == "delete" else "localDeleted"]
        saved = Path(conflict["local" if second == "edit" else "remote"])
        assert saved.read_bytes() == EDIT
        follower.disconnect()
        assert follower.status()["conflicts"] == [conflict] and saved.read_bytes() == EDIT
        winner.run("sync")
        assert proxy.writes == [operation, operation]
        if first == "edit":
            assert rows(peer_db, "SELECT revision,bytes FROM files WHERE path=?", KEY) == [(2, EDIT)]
            assert rows(peer_db, "SELECT expected,sha256 FROM receipt WHERE path=? AND operation=?", KEY, operation[1]) == [(1, sha256(EDIT).hexdigest())]
        else:
            assert rows(peer_db, "SELECT revision,bytes FROM deleted_versions WHERE path=? AND operation=?", KEY, operation[1]) == [(2, ORIGINAL)]

        follower.run("resolve", KEY, "--local" if keep == second else "--remote")
        revision = 2 if keep == first else 3
        # A second sync on each side must make no further revision or recovery changes.
        for _ in range(2):
            for client in (follower, winner):
                client.run("sync")
                current = client.status()
                assert not current["conflicts"] and not current.get("awaitingAcknowledgement")
                if keep == "edit":
                    assert client.file.read_bytes() == EDIT
                    assert rows(client.state / "client.sqlite", "SELECT revision,sha FROM baseline WHERE path=?", KEY) == [(revision, sha256(EDIT).hexdigest())]
                else:
                    assert not client.file.exists()
                    assert rows(client.state / "client.sqlite", "SELECT revision FROM deleted_baseline WHERE path=?", KEY) == [(revision,)]
            assert rows(peer_db, "SELECT revision,bytes FROM files WHERE path=?", KEY) == ([(revision, EDIT)] if keep == "edit" else [])
            archives = rows(peer_db, "SELECT revision,bytes FROM deleted_versions WHERE path=? ORDER BY revision", KEY)
            expected_archive = [(2, ORIGINAL)] if first == "delete" else ([(3, EDIT)] if keep == "delete" else [])
            assert archives == expected_archive and saved.read_bytes() == EDIT
        print(f"PASS {first} first, kept {keep}, both clients and peer at revision {revision}, recovered SHA-256 {sha256(saved.read_bytes()).hexdigest()}", flush=True)


def exercise(binary, adapter=False):
    for first in ("delete", "edit"):
        for keep in ("delete", "edit"):
            with tempfile.TemporaryDirectory(prefix="castle6-") as temp:
                scenario(Path(binary).resolve(), Path(temp).resolve(), adapter, first, keep)


class MultiClientRecovery(unittest.TestCase):
    @unittest.skipUnless(os.environ.get("COVEN_TEST_NATIVE"), "requires the native client and peer")
    def test_reconnect_and_resolve_in_both_orders(self):
        exercise(os.environ["COVEN_TEST_NATIVE"], adapter=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    args = parser.parse_args()
    exercise(args.binary)
