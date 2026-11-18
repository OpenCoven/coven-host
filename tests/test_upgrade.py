"""Exercise a stopped Python installation through its public CLI, then open it in Rust."""
import os
import sqlite3
import unittest
from unittest.mock import patch

import test_integration as reference


@unittest.skipUnless(os.environ.get("COVEN_TEST_NATIVE"), "set COVEN_TEST_NATIVE to the Rust compatibility harness")
class PythonUpgrade(unittest.TestCase):
    def test_pending_ack_and_existing_conflict_survive_both_upgrades(self):
        fixture = reference.Integration("test_round_trip_demo_and_repeatable_initialization")
        self.addCleanup(fixture.doCleanups)
        with patch.dict(os.environ, {"COVEN_TEST_CLIENT": "", "COVEN_TEST_HOST": ""}):
            fixture.setUp()
            note = fixture.castle / "files/conflict.md"
            note.write_bytes(b"common version")
            fixture.run_cli("sync")
            fixture.publish("files/conflict.md", b"host edit")
            note.write_bytes(b"client edit")
            fixture.run_cli("sync", code=1)
            pending = fixture.castle / "files/outbox.bin"
            pending.write_bytes(b"sent before upgrade")
            with fixture.proxy("ack"):
                fixture.run_cli("sync", code=1)
            fixture.stop_host()
        storage = fixture.work / "client"
        with sqlite3.connect(storage / "client.sqlite") as db:
            self.assertEqual(db.execute("SELECT path FROM pending").fetchone(), ("files/outbox.bin",))
            local, remote = db.execute("SELECT local,remote FROM conflicts").fetchone()
        copies = [storage / "recovery" / name for name in (local, remote)]
        native = os.environ["COVEN_TEST_NATIVE"]
        with patch.dict(os.environ, {"COVEN_TEST_CLIENT": native, "COVEN_TEST_HOST": native}):
            fixture.start_host()
            fixture.connect(fixture.url)
            fixture.run_cli("sync", code=1)
            fixture.assert_remote("files/outbox.bin", b"sent before upgrade", 1)
            with sqlite3.connect(storage / "client.sqlite") as db:
                self.assertEqual(db.execute("SELECT count(*) FROM pending").fetchone(), (0,))
                self.assertEqual(db.execute("SELECT path FROM conflicts").fetchone(), ("files/conflict.md",))
            fixture.run_cli("resolve", "files/conflict.md", "--local")
            fixture.run_cli("sync")
            fixture.assert_remote("files/conflict.md", b"client edit", 3)
            self.assertEqual([p.read_bytes() for p in copies], [b"client edit", b"host edit"])
