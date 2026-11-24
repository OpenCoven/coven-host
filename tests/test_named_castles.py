"""Verify named Castle isolation through the installed CLI, or run the same journey in Rust CI."""
import argparse
from contextlib import ExitStack
from hashlib import sha256
from http.server import HTTPServer
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest

from installed_deletion import stop, until, write_token
from test_multi_client import AcknowledgementProbe, Client, KEY, rows


def scenario(binary, work, adapter, conflicted):
    renamed = "demo" if conflicted == "research" else "research"
    names = ("demo", "research")
    tokens = {name: work / (name + "-access") for name in names}
    for token in tokens.values():
        write_token(token)
    owners = [{name: Client(binary, work / f"owner-{i}", adapter, name) for name in names} for i in range(2)]
    originals = {name: name.encode() + b"\x00original\r\n" for name in names}
    edit = conflicted.encode() + b"\x00offline edit\xff"
    storage = work / "peer"
    log = work / "listener.log"
    peer = None
    port = 0

    def start():
        nonlocal peer, port
        command = [binary, *([] if adapter else ["host"]), "serve", "--storage", storage,
                   "--port", str(port), "--token-file", tokens["demo"], "--castle-token", f"research={tokens['research']}"]
        with log.open("w") as output:
            peer = subprocess.Popen(command, env=owners[0]["demo"].env, stdout=output)
        until(lambda: peer.poll() is not None or "Serving Castle demo, research on " in log.read_text())
        if peer.poll() is not None:
            raise RuntimeError("Named Castle peer exited before opening its listener")
        port = int(log.read_text().splitlines()[0].rsplit(":", 1)[1])

    with ExitStack() as cleanup:
        cleanup.callback(lambda: stop(peer))
        start()
        proxy = cleanup.enter_context(HTTPServer(("127.0.0.1", 0), AcknowledgementProbe))
        proxy.upstream, proxy.writes, proxy.fault = port, [], None
        thread = threading.Thread(target=proxy.serve_forever, daemon=True)
        thread.start()
        cleanup.callback(thread.join)
        cleanup.callback(proxy.shutdown)
        url = f"http://127.0.0.1:{proxy.server_port}"
        for i, owner in enumerate(owners):
            cleanup.callback(owner["demo"].disconnect)
            for name, client in owner.items():
                client.run("init", "--root", client.root)
                client.run("connect", url, "--token-file", tokens[name])
                if i == 0:
                    client.file.write_bytes(originals[name])
                client.run("sync")
                assert client.file.read_bytes() == originals[name]
                assert client.status()["castle"] == name
        for name, other in ((conflicted, renamed), (renamed, conflicted)):
            client = owners[0][name]
            assert client.run("get", KEY, "--url", url, "--token-file", tokens[name]) == originals[name]
            client.run("get", KEY, "--url", url, "--token-file", tokens[other], success=False)
        for owner in owners:
            owner["demo"].disconnect()
        stop(peer)
        first, second = owners
        first[renamed].file.rename(first[renamed].file.with_name("moved.md"))
        first[conflicted].file.unlink()
        second[conflicted].file.write_bytes(edit)
        start()
        proxy.fault = "delete-ack"
        proxy.writes.clear()
        first[conflicted].run("sync", success=False)
        assert proxy.fault is None and len(proxy.writes) == 1
        operation, = proxy.writes
        assert operation[0] == "DELETE" and operation[1]
        assert rows(first[conflicted].state / "client.sqlite", "SELECT operation FROM pending_delete WHERE path=?", KEY) == [(operation[1],)]
        first[conflicted].disconnect()
        stop(peer)
        start()
        first[conflicted].run("sync")
        assert proxy.writes == [operation, operation]
        second[conflicted].run("sync", success=False)
        conflict, = second[conflicted].status()["conflicts"]
        assert conflict["path"] == KEY and conflict["revision"] == 2 and conflict["remoteDeleted"]
        saved = Path(conflict["local"])
        assert saved.is_relative_to(second[conflicted].state / "recovery") and saved.read_bytes() == edit
        for owner in owners:
            owner[renamed].run("sync")
            assert owner[renamed].file.with_name("moved.md").read_bytes() == originals[renamed]
            assert not owner[renamed].file.exists() and not owner[renamed].status()["conflicts"]
        versions = [Path(p) for p in second[renamed].status()["previousVersions"]]
        assert any(p.read_bytes() == originals[renamed] for p in versions)
        second[conflicted].disconnect()
        assert second[conflicted].status()["conflicts"] == [conflict]
        second[conflicted].run("resolve", KEY, "--local")
        for _ in range(2):
            for owner in reversed(owners):
                for name, client in owner.items():
                    client.run("sync")
                    status = client.status()
                    assert not status["conflicts"] and not status["awaitingAcknowledgement"]
                    expected = edit if name == conflicted else originals[renamed]
                    path = KEY if name == conflicted else "files/moved.md"
                    revision = 3 if name == conflicted else 1
                    assert client.run("get", path, "--url", url, "--token-file", tokens[name]) == expected
                    assert (client.file.parent / Path(path).name).read_bytes() == expected
                    assert rows(client.state / "client.sqlite", "SELECT revision,sha FROM baseline WHERE path=?", path) == [(revision, sha256(expected).hexdigest())]
                    peer_dir = storage if name == "demo" else storage / "castles" / name
                    assert rows(peer_dir / "host.sqlite", "SELECT revision,bytes FROM deleted_versions WHERE path=?", KEY) == [(2, originals[name])]
            assert saved.read_bytes() == edit
        print(f"PASS {conflicted} recovered revision 3; {renamed} renamed at revision 1; both deletion archives at revision 2", flush=True)


def exercise(binary, adapter=False):
    for conflicted in ("demo", "research"):
        # Darwin limits the canonical daemon socket path to 103 bytes.
        with tempfile.TemporaryDirectory(prefix="ch7-", dir="/tmp" if sys.platform == "darwin" else None) as temp:
            scenario(Path(binary).resolve(), Path(temp).resolve(), adapter, conflicted)


class NamedCastles(unittest.TestCase):
    @unittest.skipUnless(os.environ.get("COVEN_TEST_NATIVE"), "requires the native client and peer")
    def test_independent_offline_recovery(self):
        exercise(os.environ["COVEN_TEST_NATIVE"], adapter=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    exercise(parser.parse_args().binary)
