"""Protect login entries when a portable desktop installation switches versions."""
import errno
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


@unittest.skipIf(os.name == "nt", "the Linux desktop installer uses POSIX file locking")
class PortableUpgrade(unittest.TestCase):
    def setUp(self):
        source = Path(__file__).resolve().parents[1] / "installer/linux/desktop.py"
        spec = importlib.util.spec_from_file_location("coven_desktop", source)
        self.desktop = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.desktop)
        scratch = tempfile.TemporaryDirectory()
        self.addCleanup(scratch.cleanup)
        home = Path(scratch.name)
        self.app = home / "release/0.1.2"
        self.desktop.PORTABLE = self.app
        config = home / "config"
        data = home / "data"
        env = patch.dict(os.environ, XDG_CONFIG_HOME=str(config), XDG_DATA_HOME=str(data))
        env.start()
        self.addCleanup(env.stop)
        self.autostart = config / "autostart/org.opencoven.CovenHost.desktop"
        self.menu = data / "applications/org.opencoven.CovenHost.desktop"
        self.autostart.parent.mkdir(parents=True)
        self.menu.parent.mkdir(parents=True)
        # These are the published v0.1.1 portable entries, including the diagnostics action.
        script = self.app.parent / "0.1.1/desktop.py"
        self.autostart.write_text(f'[Desktop Entry]\nType=Application\nName=Coven Host sync\nTerminal=false\nExec=/usr/bin/env python3 -I "{script}" session\n')
        launch = f'/usr/bin/env python3 -I "{script}" launch'
        self.menu.write_text(f'[Desktop Entry]\nType=Application\nName=Coven Host\nIcon=folder\nTerminal=false\nCategories=Utility;FileTools;\nExec={launch}\nActions=Diagnostics;\n\n[Desktop Action Diagnostics]\nName=Show diagnostics\nExec={launch} --diagnostics\n')

    def test_upgrade_is_repeatable(self):
        for _ in range(2):
            self.desktop.register(self.app, "session", upgrading=True)
        self.assertIn(str(self.app / "desktop.py"), self.autostart.read_text())
        self.assertIn(str(self.app / "desktop.py"), self.menu.read_text())

    def test_kernel_without_pidfds_offers_relogin_without_stopping_a_process(self):
        state = self.app.parent
        state.mkdir(parents=True, exist_ok=True)
        (state / "session-ready").write_text("12345")
        with patch.object(self.desktop, "STATE", state), patch.object(self.desktop, "session_ready", return_value=True), \
             patch.object(os, "pidfd_open", side_effect=OSError(errno.ENOSYS, "unsupported"), create=True), \
             patch.object(self.desktop.signal, "pidfd_send_signal", create=True) as send:
            with self.assertRaises(self.desktop.Problem) as failure:
                self.desktop.retire_previous_service("session", None)
        self.assertEqual(failure.exception.fields[0], "LOGIN")
        self.assertIn("Sign out", failure.exception.fields[-1])
        send.assert_not_called()

    def test_foreign_menu_preserves_both_entries(self):
        previous = self.autostart.read_bytes()
        self.menu.write_text("custom desktop entry\n")
        with self.assertRaises(self.desktop.Problem):
            self.desktop.register(self.app, "session", upgrading=True)
        self.assertEqual(self.autostart.read_bytes(), previous)
        self.assertEqual(self.menu.read_text(), "custom desktop entry\n")


if __name__ == "__main__":
    unittest.main()
