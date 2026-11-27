"""Measure a single Castle through a counted loopback peer using the release adapter."""
import argparse
from collections import Counter
from contextlib import ExitStack
from http.server import HTTPServer
import json
from pathlib import Path
import subprocess
import tempfile
import threading
from time import perf_counter

from fault_proxy import FaultProxy
from installed_deletion import stop, until, write_token
from test_multi_client import Client


class Probe(FaultProxy):
    def request(self):
        self.server.calls.append((self.command, self.path))
        if self.command == "GET" and self.server.edit_on_get == len(self.server.calls):
            self.server.edit_path.write_bytes(b"edit during reconciliation")
        self.relay()

    do_GET = do_PUT = do_DELETE = request


def measure(binary, count, repeats, verify=False):
    with tempfile.TemporaryDirectory() as tmp, ExitStack() as cleanup:
        work = Path(tmp).resolve()
        clients = [Client(binary, work / "owner", True, name) for name in ("demo", "research")]
        tokens = [work / "token", work / "research-token"]
        for token in tokens:
            write_token(token)
        log = work / "peer.log"
        with log.open("w") as out:
            peer = subprocess.Popen([binary, "serve", "--storage", work / "peer", "--port", "0",
                                     "--token-file", tokens[0], "--castle-token", f"research={tokens[1]}"], stdout=out)
        cleanup.callback(stop, peer)
        until(lambda: peer.poll() is not None or "Serving Castle" in log.read_text())
        proxy = cleanup.enter_context(HTTPServer(("127.0.0.1", 0), Probe))
        proxy.upstream = int(log.read_text().splitlines()[0].rsplit(":", 1)[1])
        proxy.fault, proxy.calls, proxy.edit_on_get = None, [], None
        thread = threading.Thread(target=proxy.serve_forever, daemon=True)
        thread.start()
        cleanup.callback(thread.join)
        cleanup.callback(proxy.shutdown)
        owner, sibling = clients
        for client, token in zip(clients, tokens):
            client.run("init", "--root", client.root)
            client.run("connect", f"http://127.0.0.1:{proxy.server_port}", "--token-file", token)
        sibling.file.write_bytes(b"sibling sentinel")
        sibling.run("sync")
        for i in range(count):
            owner.file.with_name(f"item-{i}").write_bytes(bytes([i % 251]) * 8192)
        owner.run("sync")
        changed = owner.file.with_name("item-0")
        inventory = ("GET", "/v1/castles/demo/files")
        for iteration in range(repeats):
            for mode in ("unchanged", "edit", "replay"):
                if mode != "unchanged":
                    changed.write_bytes(bytes([iteration * 2 + (mode == "replay") + 1]) * 8192)
                if mode == "replay":
                    proxy.fault = "ack"
                    owner.run("sync", success=False)
                    assert proxy.fault is None
                proxy.calls.clear()
                start = perf_counter()
                owner.run("sync")
                elapsed = perf_counter() - start
                calls = Counter((m, p.split("?")[0]) for m, p in proxy.calls)
                assert all("/research/" not in p for _, p in calls)
                if verify:
                    assert calls[inventory] == (3 if mode == "replay" else 2), calls
                    assert sum(calls.values()) == calls[inventory] + (mode != "unchanged"), calls
                assert sibling.file.read_bytes() == b"sibling sentinel"
                print(json.dumps(dict(files=count, mode=mode, ms=round(elapsed * 1000, 3),
                                      calls={f"{m} {p}": n for (m,p), n in calls.items()})), flush=True)

        if verify:
            # A local writer acts after the first scan and before the last inventory returns.
            proxy.calls.clear()
            proxy.edit_on_get, proxy.edit_path = 2, changed
            assert b"files changed during reconciliation" in owner.run("sync", success=False) or (
                owner.status()["phase"] == "needs attention"
            )
            proxy.edit_on_get = None
            owner.run("sync")

            changed.unlink()
            proxy.fault = "delete-ack"
            owner.run("sync", success=False)
            assert proxy.fault is None
            proxy.calls.clear()
            owner.run("sync")
            assert Counter(proxy.calls) == Counter({
                inventory: 3,
                ("DELETE", "/v1/castles/demo/files/files/item-0"): 1,
            }), proxy.calls
            assert not changed.exists()
            assert sibling.file.read_bytes() == b"sibling sentinel"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("adapter", type=Path)
    parser.add_argument("--repeats", type=int, default=5, choices=range(1, 100))
    args = parser.parse_args()
    for size in (8, 256):
        measure(args.adapter.resolve(), size, args.repeats)
