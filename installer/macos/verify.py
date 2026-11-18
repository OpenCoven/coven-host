"""Test one extracted macOS download using only scratch homes, job labels and a private pasteboard."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import tempfile
import time


def run(*args, **kwargs):
    return subprocess.run(args, check=True, capture_output=True, timeout=75, **kwargs).stdout


def retry(probe):
    deadline = time.monotonic() + 40
    while time.monotonic() < deadline:
        try:
            if probe():
                return
        except (OSError, subprocess.CalledProcessError):
            pass
        time.sleep(0.5)
    raise RuntimeError("Automatic transfer did not finish")


def exercise(app, verifier, legacy_payload):
    with tempfile.TemporaryDirectory(prefix="ch-mac.", dir="/private/tmp") as tmp:
        home = Path(tmp)
        prefix = "org.opencoven.coven-host.test." + hashlib.sha256(tmp.encode()).hexdigest()[:12]
        domain = f"gui/{os.getuid()}"
        state = home / ".local/state/coven-host-mvp"
        agents = home / "Library/LaunchAgents"
        token = state / "access/token"
        cli = home / "Library/Application Support/Coven Host/releases/v0.1.1/bin/coven"
        env = {**os.environ, "HOME": tmp, "COVEN_HOME": str(state / "profile"), "COVEN_HOST_STATE": str(state / "client")}
        job_ids = [f"{domain}/{prefix}.{role}" for role in ["watch", "peer"]]

        def pids():
            return [re.search(rb"^\s*pid = (\d+)$", run("/bin/launchctl", "print", job), re.M).group(1) for job in job_ids]

        def unload():
            for job in job_ids:
                subprocess.run(["/bin/launchctl", "bootout", job], capture_output=True)
            if cli.exists():
                subprocess.run([cli, "daemon", "stop"], env=env, capture_output=True, timeout=30)

        try:
            download = home / "Downloads/Coven Host.app"
            run("/usr/bin/ditto", app, download)
            run(verifier, download, home)
            installed_app = home / "Applications/Coven Host.app"
            run("/usr/bin/codesign", "--verify", "--all-architectures", "--deep", "--strict", installed_app)
            assert installed_app.is_dir()
            app_inode = installed_app.stat().st_ino
            shutil.rmtree(download)
            initial_pids = pids()
            identity = token.stat().st_ino, token.read_bytes()
            assert token.stat().st_mode & 0o777 == 0o600
            item = home / "Coven/Castles/demo/files/example.txt"
            item.write_bytes(b"Coven Host watch verification\n")
            remote = "http://127.0.0.1:" + (state / "installer-port").read_text()
            api = ["--url", remote, "--token-file", token]
            retry(lambda: run(cli, "host", "get", "files/example.txt", *api, env=env) == item.read_bytes())
            run(cli, "host", "demo-result", *api, env=env)
            result = home / "Coven/Castles/demo/results/hello-from-host.md"
            retry(lambda: result.read_bytes() == run(cli, "host", "get", "results/hello-from-host.md", *api, env=env))
            run(verifier, installed_app, home)
            assert installed_app.stat().st_ino == app_inode
            assert pids() == initial_pids and (token.stat().st_ino, token.read_bytes()) == identity
            assert json.loads(run(cli, "host", "status", env=env))["phase"] == "synced"
            print("PASS durable signed app reopened after deleting the download, persistent launchd peer/watcher, automatic upload and download, repeat install with stable credentials and PIDs")

            if legacy_payload:
                unload()
                old = cli.parents[2] / "v0.1.0/node_modules/@opencoven/cli-macos/bin/coven"
                old.parent.mkdir(parents=True)
                old.write_bytes(legacy_payload.read_bytes())
                old.chmod(0o700)
                cli = old
                (state / "installer-port").unlink()
                for f in agents.glob("*.plist"):
                    config = plistlib.loads(f.read_bytes())
                    config["ProgramArguments"][0] = str(old)
                    f.write_bytes(plistlib.dumps(config))
                shutil.rmtree(installed_app)
                run(verifier, app, home)
                assert installed_app.is_dir()
                assert (token.stat().st_ino, token.read_bytes()) == identity
                assert item.read_bytes() == run(cli, "host", "get", "files/example.txt", *api, env=env)
                print("PASS resumed earlier v0.1.0 service definitions with original peer data and credential")
        finally:
            unload()

    with tempfile.TemporaryDirectory(prefix="ch-mac.", dir="/private/tmp") as tmp:
        home = Path(tmp)
        folder = home / "Coven"
        folder.mkdir()
        sentinel = folder / "owner-private-file"
        sentinel.write_text("owner-private-data")
        collision = home / "Applications/Coven Host.app"
        collision.mkdir(parents=True)
        unrelated = collision / "unrelated-application-data"
        unrelated.write_text("keep this application")
        p = subprocess.run([verifier, app, home], capture_output=True, text=True, timeout=30)
        assert p.returncode == 1 and "CH-MAC-EXISTING-SETUP" in p.stdout, (p.returncode, p.stdout)
        assert sentinel.read_text() == "owner-private-data"
        assert unrelated.read_text() == "keep this application"
        fallback = home / "Applications/Coven Host Setup.app"
        run("/usr/bin/codesign", "--verify", "--all-architectures", "--deep", "--strict", fallback)
        p = subprocess.run([verifier, fallback, home], capture_output=True, text=True, timeout=30)
        assert p.returncode == 1 and "CH-MAC-EXISTING-SETUP" in p.stdout, (p.returncode, p.stdout)
        report = (home / "diagnostics.txt").read_text()
        for field in ["0.1.1", "Platform:", "UTC time:", "Stage:", "Expected:", "Observed:", "Error classification:", "Reproduction:", "Next action:"]:
            assert field in report
        assert all(value not in report for value in [tmp, str(Path.home()), "owner-private-file", "owner-private-data"])
        print("PASS unrelated app preserved, durable fallback opened for diagnostics, existing folder preserved, copy-to-pasteboard and save diagnostics, sanitized failure fields")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("app", type=Path)
    parser.add_argument("verifier", type=Path)
    parser.add_argument("--legacy-payload", type=Path)
    args = parser.parse_args()
    exercise(args.app, args.verifier, args.legacy_payload)
