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
import sys
import tempfile
import time


def run(*args, **kwargs):
    result = subprocess.run(args, capture_output=True, timeout=75, **kwargs)
    if result.returncode and result.stdout.startswith(b"Coven Host installer "):
        # SetupFailure.report intentionally omits account paths, tokens and folder contents.
        raise RuntimeError(result.stdout.decode("utf-8", errors="replace").strip())
    result.check_returncode()
    return result.stdout


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


def exercise(app, verifier, legacy_payload, previous_app=None, previous_verifier=None, deletions=False):
    with tempfile.TemporaryDirectory(prefix="ch-mac.", dir="/private/tmp") as tmp:
        home = Path(tmp)
        prefix = "org.opencoven.coven-host.test." + hashlib.sha256(tmp.encode()).hexdigest()[:12]
        domain = f"gui/{os.getuid()}"
        state = home / ".local/state/coven-host-mvp"
        agents = home / "Library/LaunchAgents"
        token = state / "access/token"
        cli = home / "Library/Application Support/Coven Host/releases/v0.1.2/bin/coven"
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
            if previous_app:
                run(previous_verifier, previous_app, home)
                old = home / "Library/Application Support/Coven Host/releases/v0.1.1/bin/coven"
                probe = home / "Coven/Castles/demo/files/before-upgrade.txt"
                probe.write_bytes(b"before upgrade\x00\r\n")
                api = ["--url", "http://127.0.0.1:" + (state / "installer-port").read_text(), "--token-file", token]
                retry(lambda: run(old, "host", "get", "files/before-upgrade.txt", *api, env=env) == probe.read_bytes())
                identity = token.stat().st_ino, token.read_bytes()
                previous_pids = pids()
            run(verifier, download, home)
            installed_app = home / "Applications/Coven Host.app"
            run("/usr/bin/codesign", "--verify", "--all-architectures", "--deep", "--strict", installed_app)
            if previous_app:
                assert (token.stat().st_ino, token.read_bytes()) == identity
                assert pids() != previous_pids
                assert run(cli, "host", "get", "files/before-upgrade.txt", *api, env=env) == probe.read_bytes()
                for f in agents.glob("*.plist"):
                    assert plistlib.loads(f.read_bytes())["ProgramArguments"][0] == str(cli)
                assert (app / "Contents/MacOS/CovenHostSetup").read_bytes() == (installed_app / "Contents/MacOS/CovenHostSetup").read_bytes()
                print("PASS upgraded the signed v0.1.1 app, CLI, daemon and login services with exact original bytes and credential")
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
                for f in agents.glob("*.plist"):
                    assert plistlib.loads(f.read_bytes())["ProgramArguments"][0] == str(cli)
                print("PASS upgraded v0.1.0 login services with the original peer data and credential")
            if deletions:
                unload()
                sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tests"))
                from installed_deletion import exercise as check_deletions
                existing = home / "Coven", state / "server", token, int((state / "installer-port").read_text())
                check_deletions(cli, home, existing, environment=env)
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
        for field in ["0.1.2", "Platform:", "UTC time:", "Stage:", "Expected:", "Observed:", "Error classification:", "Reproduction:", "Next action:"]:
            assert field in report
        assert all(value not in report for value in [tmp, str(Path.home()), "owner-private-file", "owner-private-data"])
        print("PASS unrelated app preserved, durable fallback opened for diagnostics, existing folder preserved, copy-to-pasteboard and save diagnostics, sanitized failure fields")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("app", type=Path)
    parser.add_argument("verifier", type=Path)
    parser.add_argument("--legacy-payload", type=Path)
    parser.add_argument("--previous-app", type=Path)
    parser.add_argument("--previous-verifier", type=Path)
    parser.add_argument("--deletions", action="store_true", help="Exercise offline deletion and rename in this installer-created folder")
    args = parser.parse_args()
    if bool(args.previous_app) != bool(args.previous_verifier):
        parser.error("--previous-app and --previous-verifier are used together")
    exercise(args.app, args.verifier, args.legacy_payload, args.previous_app, args.previous_verifier, args.deletions)
