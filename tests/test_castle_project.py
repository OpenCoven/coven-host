"""An optional Project reference in castle.json survives Python-created state and upgrades.

Proposed in Coven Compass #20 and blocked on D01: the reference below is synthetic and chooses
no Project issuer or ID. These run in the third leg of tests/compat-linux.sh (COVEN_TEST_NATIVE).
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import test_integration as reference
import test_upgrade

PROJECT = {"source": "fixture.invalid/projects", "ref": "synthetic:project/one"}
NATIVE = os.environ.get("COVEN_TEST_NATIVE")


def add_project(manifest):
    value = json.loads(manifest.read_text())
    value["project"] = PROJECT
    manifest.write_text(json.dumps(value, indent=2) + "\n")
    return manifest.read_bytes()


@unittest.skipUnless(NATIVE, "set COVEN_TEST_NATIVE to the Rust compatibility harness")
class PythonCreatedManifest(unittest.TestCase):
    def test_python_manifest_with_project_opens_in_python_and_rust(self):
        from coven_host.folder import Folder  # the pinned prototype, on PYTHONPATH

        with tempfile.TemporaryDirectory() as scratch:
            base = Path(scratch).resolve()
            root, state = base / "Coven", base / "state"
            folder = Folder(root, state)
            folder.initialize()
            manifest = root / "Castles/demo/castle.json"
            for with_project in (False, True):
                before = add_project(manifest) if with_project else manifest.read_bytes()
                folder.check()
                native = subprocess.run([NATIVE, "--state", str(state), "init", "--root", str(root)],
                                        capture_output=True)
                self.assertEqual(native.returncode, 0, native.stderr)
                folder.initialize()
                self.assertEqual(manifest.read_bytes(), before)

            value = json.loads(manifest.read_text())
            value["version"] = 2
            manifest.write_text(json.dumps(value))
            with self.assertRaises(Exception):
                folder.check()
            native = subprocess.run([NATIVE, "--state", str(state), "init", "--root", str(root)],
                                    capture_output=True)
            self.assertNotEqual(native.returncode, 0, "version 2 must be refused by the Rust reader too")


original_setup = reference.Integration.setUp


def setup_with_project(fixture):
    original_setup(fixture)
    manifest = fixture.castle / "castle.json"
    before = add_project(manifest)
    # Cleanups run last-in first-out, so this check runs before the fixture removes its files.
    fixture.addCleanup(lambda: fixture.assertEqual(manifest.read_bytes(), before))


class UpgradeWithProject(test_upgrade.PythonUpgrade):
    """The unchanged upgrade tests, with a Project reference in the manifest from the start."""

    def run(self, result=None):
        with patch.object(reference.Integration, "setUp", setup_with_project):
            return super().run(result)


if __name__ == "__main__":
    unittest.main()
