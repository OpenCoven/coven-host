"""Regression tests for publishing an offline kit from qualified release files."""
import hashlib
import os
import runpy
import tempfile
import unittest
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
from pathlib import Path
from threading import Event
from unittest.mock import patch
from zipfile import ZipFile


INSTALLER = Path(__file__).resolve().parents[1] / "installer"
builder = runpy.run_path(str(INSTALLER / "package-offline.py"))
bundle = builder["bundle"]
names = builder["INSTALLERS"]


class OfflineKit(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.assets = Path(tmp.name)
        self.guide = INSTALLER / "Start-Here.html"
        self.output = self.assets / (builder["PREFIX"] + "Offline.zip")
        for name in names:
            (self.assets / name).write_bytes(name.encode())

    def build(self):
        bundle(self.assets, self.guide)

    def check_members(self):
        with ZipFile(self.output) as kit:
            self.assertEqual(kit.namelist(), ["Start-Here.html", *names, "SHA256SUMS"])
            sums = kit.read("SHA256SUMS").decode().splitlines()
            expected = [f"{hashlib.sha256(kit.read(name)).hexdigest()}  {name}"
                        for name in ["Start-Here.html", *names]]
            self.assertEqual(sums, expected)

    def test_same_bytes_after_changing_source_metadata(self):
        self.build()
        original = self.output.read_bytes()
        for name in names:
            p = self.assets / name
            os.utime(p, (1_000_000_000, 1_000_000_000))
            p.chmod(0o700)
        self.build()
        self.assertEqual(hashlib.sha256(self.output.read_bytes()).digest(),
                         hashlib.sha256(original).digest())
        self.check_members()

    def test_manifest_matches_bytes_if_an_input_is_replaced(self):
        target = self.assets / names[0]
        updated = self.assets / "replacement"
        updated.write_bytes(b"new qualified installer")
        original_open = Path.open

        @contextmanager
        def open_then_replace(path, *args, **kwargs):
            with original_open(path, *args, **kwargs) as handle:
                yield handle
            if path == target and updated.exists():
                updated.replace(target)

        with patch.object(Path, "open", open_then_replace):
            self.build()
        self.check_members()

    def test_overlapping_builds_each_publish_a_complete_kit(self):
        paused, resume = Event(), Event()
        original_open = Path.open

        def open_asset(path, *args, **kwargs):
            if path == self.assets / names[1] and not paused.is_set():
                paused.set()
                if not resume.wait(10):
                    raise TimeoutError("second build did not finish")
            return original_open(path, *args, **kwargs)

        with patch.object(Path, "open", open_asset), ThreadPoolExecutor(1) as pool:
            first = pool.submit(self.build)
            try:
                self.assertTrue(paused.wait(10))
                self.build()
                self.check_members()
            finally:
                resume.set()
            first.result()
        self.check_members()

    def test_failed_read_preserves_previous_output(self):
        self.build()
        previous = self.output.read_bytes()
        original_open = Path.open

        def fail_read(path, *args, **kwargs):
            if path == self.assets / names[1]:
                raise OSError("interrupted input")
            return original_open(path, *args, **kwargs)

        with patch.object(Path, "open", fail_read), self.assertRaisesRegex(OSError, "interrupted input"):
            self.build()
        self.assertEqual(self.output.read_bytes(), previous)
        self.assertEqual({p.name for p in self.assets.iterdir()}, {*names, self.output.name})


if __name__ == "__main__":
    unittest.main()
