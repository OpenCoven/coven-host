"""Exercise a downloaded GUI installer inside a newly created Windows test account."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

from installed_deletion import exercise, until


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
        until(lambda: (app / "local-peer/watch.pid").is_file() and cli.is_file())
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
