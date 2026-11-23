"""Exercise a downloaded GUI installer inside a newly created Windows test account."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time

from installed_deletion import exercise, until


def diagnostic_text(pid):
    # Read the rendered WinForms edit control in this disposable installer's own process.
    import ctypes
    from ctypes import wintypes

    user32 = ctypes.windll.user32
    callback = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    user32.EnumWindows.argtypes = [callback, wintypes.LPARAM]
    user32.EnumChildWindows.argtypes = [wintypes.HWND, callback, wintypes.LPARAM]
    user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
    user32.GetClassNameW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
    user32.SendMessageTimeoutW.argtypes = [
        wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM,
        wintypes.UINT, wintypes.UINT, ctypes.POINTER(ctypes.c_size_t),
    ]
    reports = []

    @callback
    def child(hwnd, _):
        name = ctypes.create_unicode_buffer(256)
        user32.GetClassNameW(hwnd, name, len(name))
        if "edit" in name.value.lower():
            text = ctypes.create_unicode_buffer(4096)
            copied = ctypes.c_size_t()
            # WM_GETTEXT is marshalled by Windows across the process boundary.
            if user32.SendMessageTimeoutW(hwnd, 0x000D, len(text), ctypes.addressof(text),
                                          2, 1000, ctypes.byref(copied)):
                if text.value.startswith("Coven Host setup diagnostic"):
                    reports.append(text.value)
        return True

    @callback
    def window(hwnd, _):
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid:
            user32.EnumChildWindows(hwnd, child, 0)
        return True

    user32.EnumWindows(window, 0)
    return next(iter(reports), None)


def wait_for_install(launcher, ready):
    end = time.monotonic() + 120
    while not ready() and time.monotonic() < end:
        if launcher.poll() is not None:
            raise RuntimeError(f"The installer exited with code {launcher.returncode}")
        report = diagnostic_text(launcher.pid)
        if report:
            raise RuntimeError(report)
        time.sleep(0.5)
    if not ready():
        raise RuntimeError("The installer did not display a diagnostic or start its watcher within 120 seconds")


def stop_registered(cli, state):
    # Stop only the processes the fresh installer launched, after verifying each executable and start time.
    for role in ("watch", "peer"):
        pid, created = (state / (role + ".pid")).read_text().split(",")
        if not pid.isdecimal() or not created.isdecimal():
            raise RuntimeError("Invalid installer process record")
        script = """$p = Get-Process -Id $env:QA_PID -ErrorAction Stop
if ($p.Path -ne $env:QA_CLI -or $p.StartTime.ToUniversalTime().Ticks -ne [long]$env:QA_CREATED) { exit 1 }
$p.Kill()
$p.WaitForExit()
"""
        subprocess.run(["powershell.exe", "-NoProfile", "-Command", script], check=True,
                       env=dict(os.environ, QA_PID=pid, QA_CREATED=created, QA_CLI=str(cli)))
    subprocess.run([cli, "daemon", "stop"], check=True, capture_output=True, timeout=30)


def main():
    if sys.argv[1:2] == ["--group-owned"]:
        if os.name != "nt" or os.environ.get("GITHUB_ACTIONS") != "true":
            raise RuntimeError("Run this check on the disposable Windows runner")
        owner = subprocess.check_output(["powershell.exe", "-NoProfile", "-Command",
            "$i = [Security.Principal.WindowsIdentity]::GetCurrent(); $i.Owner -eq $i.User"], text=True).strip()
        if owner != "False":
            raise RuntimeError("The negative control needs a group-owned Windows token")
        application = Path(os.environ["LOCALAPPDATA"]) / "OpenCoven/Coven Host"
        root = Path.home() / "Coven"
        assert not application.exists() and not root.exists()
        with subprocess.Popen([Path(sys.argv[2]).resolve(strict=True)]) as launcher:
            try:
                wait_for_install(launcher, lambda: False)
            except RuntimeError as exc:
                text = str(exc)
                if "Code: CH-W12" not in text or "File Explorer" not in text:
                    raise
            finally:
                launcher.terminate()
        assert not application.exists() and not root.exists()
        print("PASS group-owned Windows session displayed CH-W12 before creating installation or recovery files")
        return
    if os.name != "nt" or not re.fullmatch(r"CovenQA[0-9a-f]{8}", os.environ["USERNAME"]):
        raise RuntimeError("Run in the disposable Windows qualification account")
    if "S-1-16-8192" not in subprocess.check_output(["whoami", "/groups"], text=True):
        raise RuntimeError("Qualification requires a standard Windows user token")
    package = Path(sys.argv[1]).resolve(strict=True)
    root = Path.home() / "Coven"
    app = Path(os.environ["LOCALAPPDATA"]) / "OpenCoven/Coven Host"
    if root.exists() or app.exists():
        raise RuntimeError("Qualification requires a fresh Windows account")
    cli = app / "0.1.2/coven.exe"
    state = app / "local-peer"
    previous_token = None
    if len(sys.argv) == 3:
        previous = app / "0.1.1/coven.exe"
        old_setup = subprocess.Popen([Path(sys.argv[2]).resolve(strict=True)])
        try:
            wait_for_install(old_setup, lambda: (state / "watch.pid").is_file() and previous.is_file())
            item = root / "Castles/demo/files/pre-upgrade.txt"
            item.write_bytes(b"prior release bytes\x00\r\n")
            api = ["--url", "http://127.0.0.1:" + (state / "port").read_text(), "--token-file", state / "access.token"]
            until(lambda: subprocess.run([previous, "host", "get", "files/pre-upgrade.txt", *api],
                                        capture_output=True, timeout=15).stdout == item.read_bytes())
            previous_token = (state / "access.token").read_bytes()
            stop_registered(previous, state)
            (state / "watch.pid").unlink(missing_ok=True)
        finally:
            if old_setup.poll() is None:
                old_setup.terminate()
                old_setup.wait(timeout=15)
    launcher = subprocess.Popen([package])
    try:
        wait_for_install(launcher, lambda: (app / "local-peer/watch.pid").is_file() and cli.is_file())
        def online():
            p = subprocess.run([cli, "host", "status"], capture_output=True, timeout=15)
            if p.returncode:
                return False
            status = json.loads(p.stdout)
            return Path(status["root"]) == root and status["phase"] == "synced"
        until(online)
        print("PASS downloaded GUI installer created the ordinary Castle folder and started its peer and watcher", flush=True)
        if previous_token is not None:
            assert previous_token == (state / "access.token").read_bytes()
            assert subprocess.check_output([cli, "host", "get", "files/pre-upgrade.txt", *api]) == item.read_bytes()
            print("PASS upgraded the v0.1.1 Windows installation after its login services stopped, preserving folder, exact bytes and token", flush=True)
        stop_registered(cli, state)
        installation = root, state / "server", state / "access.token", int((state / "port").read_text())
        with tempfile.TemporaryDirectory(prefix="installed-castle-") as fixture:
            exercise(cli, Path(fixture).resolve(), installation)
    finally:
        if launcher.poll() is None:
            launcher.terminate()
            launcher.wait(timeout=15)


if __name__ == "__main__":
    main()
