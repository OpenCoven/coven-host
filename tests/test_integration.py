import contextlib
from hashlib import sha256
import http.client
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
from pathlib import Path
import secrets
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

from coven_host.common import MAX_FILE, Problem, digest
from coven_host.folder import Folder, exchange
from coven_host.host import PREFIX


REPO = Path(__file__).resolve().parents[1]


def executable(variable):
    binary = os.environ.get(variable)
    return [binary] if binary else [sys.executable, "-m", "coven_host"]


class FaultProxy(BaseHTTPRequestHandler):
    def relay(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        conn = http.client.HTTPConnection("127.0.0.1", self.server.upstream, timeout=5)
        try:
            conn.request(self.command, self.path, body, dict(self.headers))
            with conn.getresponse() as response:
                data = response.read()
                if (self.command, self.server.fault) in {("PUT", "ack"), ("DELETE", "delete-ack")}:
                    self.server.fault = None
                    return  # Drop the response after the upstream commit.
                self.send_response(response.status)
                self.send_header("Content-Length", str(len(data)))
                if response.getheader("X-Revision"):
                    self.send_header("X-Revision", response.getheader("X-Revision"))
                    if self.server.fault == "download":
                        self.server.fault = None
                        data = data[:1]
                self.end_headers()
                self.wfile.write(data)
        finally:
            conn.close()

    do_GET = do_PUT = do_DELETE = relay

    def log_message(self, *_):
        pass


class Integration(unittest.TestCase):
    def setUp(self):
        work = tempfile.TemporaryDirectory()
        self.addCleanup(work.cleanup)
        self.work = Path(work.name).resolve()
        self.token = self.work / "access"
        self.token.write_text(secrets.token_urlsafe(32) + "\n")
        self.token.chmod(0o600)
        self.root = self.work / "Coven"
        self.castle = self.root / "Castles" / "demo"
        self.port = 0
        self.start_host()
        self.addCleanup(self.stop_host)
        self.run_cli("init", "--root", self.root)
        self.connect(self.url)

    def start_host(self):
        log_path = self.work / "server.log"
        with log_path.open("w") as log:
            self.host = subprocess.Popen(
                [*executable("COVEN_TEST_HOST"), "serve", "--storage", str(self.work / "server"),
                 "--token-file", str(self.token), "--port", str(self.port)],
                cwd=REPO, stdout=log, stderr=subprocess.STDOUT,
            )
        for _ in range(100):
            output = log_path.read_text()
            if output.startswith("Serving Castle demo on"):
                self.port = int(output.splitlines()[0].rsplit(":", 1)[1])
                self.url = f"http://127.0.0.1:{self.port}"
                return
            if self.host.poll() is not None:
                break
            time.sleep(0.03)
        self.stop_host()
        self.fail(f"host failed to start: {output}")

    def stop_host(self):
        if self.host.poll() is None:
            self.host.kill()
            self.host.wait(timeout=5)

    def run_cli(self, *args, code=0):
        result = subprocess.run(
            [*executable("COVEN_TEST_CLIENT"), "--state", str(self.work / "client"),
             *map(str, args)], cwd=REPO, capture_output=True, timeout=15,
        )
        self.assertEqual(result.returncode, code, result.stdout + result.stderr)
        return result.stdout + result.stderr

    def connect(self, url):
        return self.run_cli("connect", url, "--token-file", self.token)

    def host_cli(self, command, *args):
        return self.run_cli(command, "--url", self.url, "--token-file", self.token, *args)

    def publish(self, path, data):
        source = self.work / "source"
        source.write_bytes(data)
        return self.host_cli("put", path, source)

    def request(self, method, route=PREFIX, headers=None, body=None, auth=True):
        headers = dict(headers or {})
        if auth:
            headers.setdefault("Authorization", "Bearer " + self.token.read_text().strip())
        conn = http.client.HTTPConnection("127.0.0.1", self.port, timeout=5)
        try:
            conn.request(method, route, body, headers)
            with conn.getresponse() as reply:
                return reply.status, reply.read()
        finally:
            conn.close()

    def inventory(self):
        status, body = self.request("GET")
        self.assertEqual(status, 200)
        return json.loads(body)

    def assert_remote(self, path, data, revision):
        listed = {row["path"]: row for row in self.inventory()["files"]}
        self.assertEqual(listed[path], {"path": path, "revision": revision,
                                       "sha256": sha256(data).hexdigest(), "size": len(data)})
        self.assertEqual(self.request("GET", PREFIX + "/" + path), (200, data))

    @contextlib.contextmanager
    def proxy(self, fault):
        with HTTPServer(("127.0.0.1", 0), FaultProxy) as proxy:
            proxy.upstream, proxy.fault = self.port, fault
            thread = threading.Thread(target=proxy.serve_forever)
            thread.start()
            try:
                self.connect(f"http://127.0.0.1:{proxy.server_port}")
                yield proxy
                self.assertIsNone(proxy.fault)
            finally:
                proxy.shutdown()
                thread.join(timeout=5)

    def test_round_trip_demo_and_repeatable_initialization(self):
        path = self.castle / "files" / "note.bin"
        path.write_bytes(b"\x00hello\xff")
        (self.root / "README.md").write_text("owner's notes")
        self.run_cli("init", "--root", self.root)
        self.assertEqual((self.root / "README.md").read_text(), "owner's notes")
        for revision, data in enumerate((b"\x00hello\xff", b"edited\x00docs"), 1):
            path.write_bytes(data)
            self.run_cli("sync")
            self.assert_remote("files/note.bin", data, revision)
        self.host_cli("demo-result")
        demo = self.castle / "results" / "hello-from-host.md"
        self.run_cli("sync")
        self.assertIn(b"Castle: `demo`", demo.read_bytes())
        self.assertIn(b"Demo message:", demo.read_bytes())
        self.assert_remote("results/hello-from-host.md", demo.read_bytes(), 1)
        for data in (b"host first", b"updated on host\x00"):
            self.publish("results/nested/report.bin", data)
            self.run_cli("sync")
            self.assertEqual(sha256((self.castle / "results/nested/report.bin").read_bytes()).digest(),
                             sha256(data).digest())
        status = self.run_cli("status")
        for item in (b"Status: synced", b"Pending local paths: 0", b"Last successful contact: 20"):
            self.assertIn(item, status)

    def test_offline_restart_and_watch(self):
        path = self.castle / "files/note.md"
        path.write_bytes(b"before")
        self.run_cli("sync")
        self.stop_host()
        path.write_bytes(b"offline")
        self.assertIn(b"offline:", self.run_cli("sync", code=1))
        status = self.run_cli("status")
        self.assertIn(b"Status: offline", status)
        self.assertIn(b"Pending local paths: 1", status)
        self.assertEqual(path.read_bytes(), b"offline")
        self.start_host()
        self.assert_remote("files/note.md", b"before", 1)
        with subprocess.Popen([*executable("COVEN_TEST_CLIENT"), "--state", str(self.work / "client"),
                               "watch"], cwd=REPO, stdout=subprocess.PIPE) as watch:
            try:
                for _ in range(100):
                    if b"Status: synced" in self.run_cli("status"):
                        break
                    time.sleep(0.03)
                else:
                    self.fail("foreground watch did not reconcile after restart")
                self.assert_remote("files/note.md", b"offline", 2)
            finally:
                watch.terminate()
                watch.wait(timeout=5)

    def test_conflict_retention_and_all_resolution_modes(self):
        for choice, selected in (("local", b"local"), ("remote", b"remote"), ("merged", b"merged")):
            with self.subTest(choice=choice):
                path = self.castle / f"files/{choice}.md"
                key = f"files/{choice}.md"
                path.write_bytes(b"baseline")
                self.run_cli("sync")
                self.publish(key, b"remote")
                path.write_bytes(b"local")
                self.assertIn(b"unresolved conflicts", self.run_cli("sync", code=1))
                box = self.work / "client" / "recovery"
                copies = {entry: entry.read_bytes() for entry in box.iterdir()}
                self.assertIn(b"local", copies.values())
                self.assertIn(b"remote", copies.values())
                self.run_cli("sync", code=1)
                self.assertEqual(set(box.iterdir()), set(copies))
                self.assertIn(b"Conflicts: 1", self.run_cli("status"))
                merged = self.work / "merged"
                merged.write_bytes(selected)
                args = [merged] if choice == "merged" else []
                self.run_cli("resolve", key, "--" + choice, *args)
                self.run_cli("sync")
                self.assertEqual(path.read_bytes(), selected)
                self.assert_remote(key, selected, 2 if choice == "remote" else 3)
                self.assertEqual({entry: entry.read_bytes() for entry in box.iterdir()}, copies)

    def test_lost_commit_ack_then_interrupted_download(self):
        path = self.castle / "files/note.md"
        path.write_bytes(b"first upload")
        with self.proxy("ack"):
            self.run_cli("sync", code=1)
            self.assertIn(b"Awaiting acknowledgement: files/note.md", self.run_cli("status"))
        self.assert_remote("files/note.md", b"first upload", 1)
        self.stop_host()
        self.start_host()
        self.publish("files/note.md", b"new host version after the lost acknowledgement")
        with self.proxy("download"):
            self.run_cli("sync", code=1)
            self.assertEqual(path.read_bytes(), b"first upload")
        self.connect(self.url)
        self.run_cli("sync")
        self.assertEqual(path.read_bytes(), b"new host version after the lost acknowledgement")
        self.assert_remote("files/note.md", path.read_bytes(), 2)
        self.assertIn(b"Conflicts: 0", self.run_cli("status"))
        self.assertEqual(list((self.work / "client" / "recovery").iterdir()), [])

    @unittest.skipIf(os.environ.get("COVEN_TEST_CLIENT"), "Rust replacement race runs in the native folder tests")
    def test_mid_replace_edit_is_retained_and_reported(self):
        path = self.castle / "files/note.md"
        path.write_bytes(b"before")
        folder = Folder(self.root, self.work / "client")

        def editor_saves(*args):
            path.write_bytes(b"racing edit")
            exchange(*args)

        with patch("coven_host.folder.exchange", side_effect=editor_saves):
            with self.assertRaisesRegex(Problem, "edit raced replacement"):
                folder.replace("files/note.md", b"download", digest(b"before"))
        self.assertEqual(path.read_bytes(), b"download")
        copies = list((self.work / "client" / "recovery").iterdir())
        self.assertEqual([copy.read_bytes() for copy in copies], [b"racing edit"])
        self.assertIn(str(copies[0]).encode(), self.run_cli("status"))
        self.assertIn(b"interrupted or raced replacement", self.run_cli("sync", code=1))

    def test_host_rollback_does_not_replace_local_bytes(self):
        path = self.castle / "files/note.md"
        path.write_bytes(b"old")
        self.run_cli("sync")
        self.stop_host()
        host_db = self.work / "server" / "host.sqlite"
        backup = self.work / "backup"
        shutil.copyfile(host_db, backup)
        self.start_host()
        path.write_bytes(b"new")
        self.run_cli("sync")
        self.stop_host()
        shutil.copyfile(backup, host_db)
        self.start_host()
        self.assertIn(b"rolled back", self.run_cli("sync", code=1))
        self.assertEqual(path.read_bytes(), b"new")

    def test_topology_collision_does_not_block_unrelated_files(self):
        blocked = self.castle / "files/blocked"
        blocked.write_bytes(b"local")
        self.publish("files/blocked/remote.md", b"remote")
        (self.castle / "files/regular.md").write_bytes(b"other")
        self.assertIn(b"file/directory collision", self.run_cli("sync", code=1))
        self.assert_remote("files/regular.md", b"other", 1)
        blocked.rename(blocked.with_name("moved"))
        self.run_cli("sync")
        self.assertEqual((blocked / "remote.md").read_bytes(), b"remote")
        self.assert_remote("files/moved", b"local", 1)

    def test_authentication_boundaries_and_upload_limits(self):
        for method, route in (("GET", PREFIX), ("GET", PREFIX + "/files/file.md"),
                              ("PUT", PREFIX + "/files/file.md")):
            self.assertEqual(self.request(method, route, auth=False)[0], 401)
            wrong = "Bearer " + secrets.token_urlsafe(32)
            self.assertEqual(self.request(method, route, {"Authorization": wrong})[0], 401)
        for suffix in ("/files/../secret", "/files/%2e%2e/secret", "/files//secret",
                       "/files/UPPER", "/files/.env.local", "/files/a%2fb"):
            with self.subTest(suffix=suffix):
                self.assertEqual(self.request("GET", PREFIX + suffix)[0], 400)
        self.assertEqual(self.request("GET", "/v1/castles/other/files")[0], 404)
        pin = self.inventory()["host"]
        headers = {"X-Host-ID": pin, "If-Match": '"0"', "X-Content-SHA256": sha256(b"").hexdigest()}
        route = PREFIX + "/files/file.bin"
        self.assertEqual(self.request("PUT", route, {**headers, "Content-Length": str(MAX_FILE + 1)}, b"")[0], 413)
        self.assertEqual(self.request("PUT", route, headers, b"does not match hash")[0], 400)
        self.assertEqual(self.inventory()["files"], [])
        target = self.work / "outside"
        target.write_bytes(b"do not touch")
        link = self.castle / "files/file.bin"
        link.symlink_to(target)
        self.publish("files/file.bin", b"remote")
        self.assertIn(b"no symlinks", self.run_cli("sync", code=1))
        self.assertEqual(target.read_bytes(), b"do not touch")
        self.assertTrue(link.is_symlink())
        link.unlink()
        (self.castle / "results").rmdir()
        (self.castle / "results").symlink_to(self.work, target_is_directory=True)
        self.assertIn(b"root unavailable or unsafe", self.run_cli("sync", code=1))

    @unittest.skipUnless(os.environ.get("COVEN_TEST_NATIVE"), "requires deletion-capable Rust endpoints")
    def test_offline_rename_after_peer_restart(self):
        original = self.castle / "files/note.md"
        renamed = original.with_name("renamed.md")
        original.write_bytes(b"offline rename")
        self.run_cli("sync")
        self.stop_host()
        original.rename(renamed)
        self.start_host()
        self.run_cli("sync")
        self.assert_remote("files/renamed.md", b"offline rename", 1)
        self.assertEqual(self.request("GET", PREFIX + "/files/note.md")[0], 404)
        self.assertFalse(original.exists())
        self.assertEqual(renamed.read_bytes(), b"offline rename")

    @unittest.skipUnless(os.environ.get("COVEN_TEST_NATIVE"), "requires deletion-capable Rust endpoints")
    def test_peer_rename_retains_displaced_local_file_after_restart(self):
        key = "files/note.md"
        original = self.castle / key
        data = b"keep this file in recovery"
        self.publish(key, data)
        self.run_cli("sync")
        self.publish("files/moved.md", data)
        headers = {"X-Host-ID": self.inventory()["host"], "If-Match": '"1"',
                   "X-Sync-Operation": secrets.token_hex(16)}
        status, body = self.request("DELETE", PREFIX + "/" + key, headers)
        self.assertEqual(status, 200, body)
        self.stop_host()
        self.start_host()
        self.run_cli("sync")
        self.assertFalse(original.exists())
        self.assertEqual((self.castle / "files/moved.md").read_bytes(), data)
        retained = list((self.work / "client/previous-versions/files/note.md").glob(".version-*"))
        self.assertEqual([p.read_bytes() for p in retained], [data])
        self.run_cli("sync")

    @unittest.skipUnless(os.environ.get("COVEN_TEST_NATIVE"), "requires deletion-capable Rust endpoints")
    def test_delete_retry_after_lost_ack_and_restart(self):
        local = self.castle / "files/note.md"
        local.write_bytes(b"recoverable")
        self.run_cli("sync")
        local.unlink()
        with self.proxy("delete-ack"):
            self.run_cli("sync", code=1)
        self.stop_host()
        self.start_host()
        self.connect(self.url)
        self.run_cli("sync")
        status, body = self.request("GET", headers={"X-Coven-Deletions": "1"})
        self.assertEqual(status, 200)
        self.assertEqual(json.loads(body)["deleted"], {"files/note.md": 2})
        self.assertFalse(local.exists())
        self.assertEqual(self.request("GET", PREFIX + "/files/note.md")[0], 404)

    @unittest.skipUnless(os.environ.get("COVEN_TEST_NATIVE"), "requires deletion-capable Rust endpoints")
    def test_deleted_path_rejects_peer_rollback(self):
        local = self.castle / "files/note.md"
        local.write_bytes(b"before deletion")
        self.run_cli("sync")
        self.stop_host()
        database = self.work / "server/host.sqlite"
        snapshot = self.work / "snapshot.sqlite"
        shutil.copyfile(database, snapshot)
        self.start_host()
        local.unlink()
        self.run_cli("sync")
        self.stop_host()
        shutil.copyfile(snapshot, database)
        self.start_host()
        self.assertIn(b"history missing or rolled back", self.run_cli("sync", code=1))
        self.assertFalse(local.exists())
        self.assert_remote("files/note.md", b"before deletion", 1)

    @unittest.skipUnless(os.environ.get("COVEN_TEST_NATIVE"), "requires deletion-capable Rust endpoints")
    def test_delete_edit_conflict_resolves_either_side(self):
        for deleted in ("local", "remote"):
            for choice in ("local", "remote"):
                with self.subTest(deleted=deleted, choice=choice):
                    key = f"files/{deleted}-{choice}.md"
                    path = self.castle / key
                    self.publish(key, b"base")
                    self.run_cli("sync")
                    edit = key.encode()
                    if deleted == "local":
                        path.unlink()
                        self.publish(key, edit)
                    else:
                        path.write_bytes(edit)
                        headers = {"X-Host-ID": self.inventory()["host"], "If-Match": '"1"',
                                   "X-Sync-Operation": secrets.token_hex(16)}
                        self.assertEqual(self.request("DELETE", PREFIX + "/" + key, headers)[0], 200)
                    self.assertIn(b"unresolved conflicts", self.run_cli("sync", code=1))
                    self.assertIn(b"Conflicts: 1", self.run_cli("status"))
                    self.run_cli("resolve", key, "--" + choice)
                    self.run_cli("sync")
                    if choice == deleted:
                        self.assertFalse(path.exists())
                        self.assertEqual(self.request("GET", PREFIX + "/" + key)[0], 404)
                    else:
                        self.assertEqual(path.read_bytes(), edit)
                        self.assert_remote(key, edit, 3 if deleted == "remote" else 2)
                    copies = (self.work / "client/recovery").glob("*-" + sha256(edit).hexdigest())
                    self.assertIn(edit, [copy.read_bytes() for copy in copies])

    def test_exclusions_local_limit_and_deletion_boundary(self):
        files = self.castle / "files"
        for name in (".git", ".env.local", "private.key", "credentials.json", "wait.tmp", "node_modules"):
            (files / name).write_bytes(b"must remain local")
        oversized = files / "large.bin"
        with oversized.open("wb") as file:
            file.truncate(MAX_FILE + 1)
        self.assertIn(b"exceeds", self.run_cli("sync", code=1))
        self.assertEqual(self.inventory()["files"], [])
        oversized.unlink()
        note = files / "note.md"
        note.write_bytes(b"keep on host")
        self.run_cli("sync")
        self.assertEqual([row["path"] for row in self.inventory()["files"]], ["files/note.md"])
        note.rename(files / "renamed.md")
        if os.environ.get("COVEN_TEST_NATIVE"):
            self.run_cli("sync")
            self.assertEqual(self.request("GET", PREFIX + "/files/note.md")[0], 404)
        else:
            self.assertIn(b"deletion propagation unsupported", self.run_cli("sync", code=1))
            self.assert_remote("files/note.md", b"keep on host", 1)
        self.assertFalse(note.exists())
        self.assert_remote("files/renamed.md", b"keep on host", 1)
        self.root.rename(self.work / "unmounted")
        self.assertIn(b"root unavailable or unsafe", self.run_cli("sync", code=1))
        status = self.run_cli("status")
        self.assertIn(b"Status: needs attention", status)
        self.assertIn(b"Pending local paths: unknown", status)
        self.assert_remote("files/renamed.md", b"keep on host", 1)

    def test_aggregate_quota_is_enforced_by_both_sides(self):
        body = b"\0" * MAX_FILE
        headers = {"X-Host-ID": self.inventory()["host"], "If-Match": '"0"',
                   "X-Content-SHA256": sha256(body).hexdigest()}
        for i in range(10):
            route = PREFIX + f"/files/f{i}.bin"
            self.assertEqual(self.request("PUT", route, headers, body)[0], 200)
        route = PREFIX + "/files/excess.bin"
        self.assertEqual(self.request("PUT", route, headers, body)[0], 413)
        self.assertEqual(self.request("GET", route)[0], 404)
        self.assertEqual(len(self.inventory()["files"]), 10)
        for i in range(11):
            with (self.castle / f"files/local{i}.bin").open("wb") as file:
                file.truncate(MAX_FILE)
        self.assertIn(b"local Castle exceeds 100 MiB", self.run_cli("sync", code=1))
        self.assertEqual(len(self.inventory()["files"]), 10)


if __name__ == "__main__":
    unittest.main()
