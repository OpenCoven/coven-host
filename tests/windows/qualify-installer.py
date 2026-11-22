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


def wait_for_install(launcher, ready):
    # Read only the sanitized report field in the installer window belonging to this fresh account.
    script = r"""Add-Type -AssemblyName UIAutomationClient
$p = Get-Process -Id $env:QA_SETUP_PID -ErrorAction Stop
if ($p.MainWindowHandle -ne 0) {
    $form = [System.Windows.Automation.AutomationElement]::FromHandle($p.MainWindowHandle)
    $condition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Edit)
    foreach ($edit in $form.FindAll([System.Windows.Automation.TreeScope]::Descendants, $condition)) {
        $value = $null
        if ($edit.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$value)) {
            $text = $value.Current.Value
            if ($text.StartsWith('Coven Host setup diagnostic')) { Write-Output $text }
        }
    }
}"""
    end = time.monotonic() + 120
    while not ready() and time.monotonic() < end:
        if launcher.poll() is not None:
            raise RuntimeError(f"The installer exited with code {launcher.returncode}")
        observation = subprocess.run(["powershell.exe", "-NoProfile", "-Command", script], capture_output=True,
                                     text=True, timeout=15, env=dict(os.environ, QA_SETUP_PID=str(launcher.pid)))
        if observation.stdout.startswith("Coven Host setup diagnostic"):
            raise RuntimeError(observation.stdout.strip())
        time.sleep(1)
    if not ready():
        raise RuntimeError("The installer did not publish its watcher process record within 120 seconds")


def main():
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
        state = app / "local-peer"
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
        installation = root, state / "server", state / "access.token", int((state / "port").read_text())
        with tempfile.TemporaryDirectory(prefix="installed-castle-") as fixture:
            exercise(cli, Path(fixture).resolve(), installation)
    finally:
        if launcher.poll() is None:
            launcher.terminate()
            launcher.wait(timeout=15)


if __name__ == "__main__":
    main()
