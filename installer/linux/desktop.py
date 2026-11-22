#!/usr/bin/env python3
"""First launch and per-user service for the bundled Coven Host CLI."""
import datetime
import errno
import fcntl
import hashlib
import json
import os
import platform
import re
import secrets
import shutil
import signal
import socket
import sqlite3
import stat
import subprocess
import sys
import tempfile
import time
import traceback
from pathlib import Path

VERSION = "0.1.2"
HOME = Path.home()
ROOT = HOME / "Coven"
STATE = HOME / ".local/state/coven-host-desktop"
SUPPORT = HOME / ".local/state/coven-host-support"
TOKEN = STATE / "access-token"
DB = STATE / "client/client.sqlite"
HERE = Path(__file__).resolve().parent
UNIT = "coven-host-desktop.service"
SYSTEM = Path("/usr/lib/coven-host-desktop")
PORTABLE = HOME / ".local/share/coven-host-desktop" / VERSION
RETRY = "Open Coven Host again. If this repeats, share this report with support."
DB_HELP = "Restore the original private Coven Host state from your backup, then open Coven Host again."


class Problem(Exception):
    def __init__(self, code, stage, expected, actual, next_step, detail=None):
        self.detail = detail
        self.fields = (code, stage, expected, actual, next_step)


def require(ok, *fields):
    if not ok:
        raise Problem(*fields)


def regular(path):
    return not path.is_symlink() and path.is_file()


def present(path):
    return os.path.lexists(path)


def private(path):
    path.mkdir(mode=0o700, parents=True, exist_ok=True)
    info = path.lstat()
    require(stat.S_ISDIR(info.st_mode) and info.st_uid == os.getuid() and info.st_mode & 0o077 == 0,
            "PRIVACY", "check storage", "a private user-owned directory", "permissions or owner differ",
            "Check ownership and owner-only access on the Coven Host private state directory, then reopen the app.")


def report(problem):
    code, stage, expected, actual, step = problem.fields
    kernel = re.match(r"[0-9]+(?:\.[0-9]+){1,3}", platform.release())
    libc = glibc_version() or "unavailable"
    python = ".".join(map(str, sys.version_info[:3]))
    detail = ("Process detail: " + problem.detail + "\n") if problem.detail else ""
    reproduce = "Open Coven Host from Applications or run its launcher again in your user session."
    if code in ("SYNC", "OFFLINE", "CONFLICT", "OK"):
        reproduce = "Open Coven Host, then use the Show diagnostics menu action or run the launcher with --diagnostics."
    return (f"Coven Host • {step}\n\nCode: CH-LINUX-{code}\nStage: {stage}\nVersion: {VERSION}\n"
            f"Time (UTC): {datetime.datetime.now(datetime.timezone.utc).isoformat(timespec='seconds')}\n"
            f"Platform: Linux\nArchitecture: {platform.machine()}\nKernel: {kernel[0] if kernel else 'unavailable'}\n"
            f"C runtime: {libc}\nPython: {python}\nExpected: {expected}\nActual: {actual}\n{detail}"
            f"Reproduce: {reproduce}\nNext step: {step}\n")


def write_file(path, data):
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as temp:
        try:
            temp.write(data)
            temp.flush()
            os.fsync(temp.fileno())
            os.replace(temp.name, path)
        finally:
            if os.path.exists(temp.name):
                os.unlink(temp.name)


def save_report(body):
    try:
        private(SUPPORT)
        path = SUPPORT / "last-error.txt"
        write_file(path, body.encode())
        return path
    except (OSError, Problem):
        try:
            with tempfile.NamedTemporaryFile(prefix="coven-host-diagnostic-", suffix=".txt", delete=False) as tmp:
                tmp.write(body.encode())
                return Path(tmp.name)
        except OSError:
            return None


def has_display():
    return bool(os.environ.get("DISPLAY") or os.environ.get("WAYLAND_DISPLAY"))


def open_with_desktop(path):
    for cmd in (["xdg-open"], ["gio", "open"]):
        if shutil.which(cmd[0]):
            # File managers can remain attached to xdg-open for the whole session.
            proc = subprocess.Popen([*cmd, path], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                    stdin=subprocess.DEVNULL, start_new_session=True)
            try:
                if proc.wait(timeout=2) == 0:
                    return True
            except subprocess.TimeoutExpired:
                return True
    return False


def display(body, path):
    print(body, file=sys.stderr, flush=True)
    if path == SUPPORT / "last-error.txt":
        print("Report saved in ~/.local/state/coven-host-support/last-error.txt", file=sys.stderr)
    elif path:
        print("Report saved in the temporary directory as " + path.name, file=sys.stderr)
    if path and has_display():
        if shutil.which("zenity"):
            cmd = ["zenity", "--text-info", "--width=680", "--height=470", "--title=Coven Host report", "--filename", path]
            clipboard = None
            if os.environ.get("WAYLAND_DISPLAY") and shutil.which("wl-copy"):
                clipboard = ["wl-copy", "--type", "text/plain"]
            elif os.environ.get("DISPLAY") and shutil.which("xclip"):
                clipboard = ["xclip", "-selection", "clipboard", "-in"]
            elif os.environ.get("DISPLAY") and shutil.which("xsel"):
                clipboard = ["xsel", "--clipboard", "--input"]
            if clipboard:
                cmd.append("--extra-button=Copy report")
            choice = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
            if clipboard and choice.stdout.strip() == b"Copy report":
                copied = subprocess.run(clipboard, input=body.encode(), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                if copied.returncode == 0:
                    return
            elif choice.returncode in (0, 1):
                return
        if shutil.which("kdialog"):
            if subprocess.run(["kdialog", "--title", "Coven Host report", "--textbox", path], stderr=subprocess.DEVNULL).returncode in (0, 1):
                return
        open_with_desktop(path)


def glibc_version():
    try:
        value = os.confstr("CS_GNU_LIBC_VERSION") or ""
    except (OSError, ValueError):
        return ""
    return value if re.fullmatch(r"glibc [0-9]+(?:\.[0-9]+){1,2}", value) else ""


def verify(app):
    info = json.loads((app / "platform.json").read_text())
    require(platform.system() == "Linux" and platform.machine() == info["machine"],
            "ARCH", "check platform", "Linux " + info["machine"], "different platform or CPU",
            "Download the Coven Host package that matches your computer, then open it again.")
    libc = glibc_version()
    required = tuple(map(int, info["glibc"].split(".")))
    actual = libc.partition(" ")[2].split(".")
    require(libc.startswith("glibc ") and all(n.isdigit() for n in actual) and tuple(map(int, actual)) >= required,
            "LIBC", "check Linux runtime", "glibc " + info["glibc"] + " or later", "unsupported C runtime",
            "Use a supported glibc-based Linux installation, then reopen Coven Host.")
    require(regular(app / "coven") and hashlib.sha256((app / "coven").read_bytes()).hexdigest() == info["sha256"],
            "INTEGRITY", "verify the installed program", "qualified executable checksum", "missing or changed program",
            "Download the installer again from the Coven Host release and reinstall it.")


def copy_portable():
    # Publish the complete version together. Existing versions must exactly match this release.
    PORTABLE.parent.mkdir(parents=True, exist_ok=True)
    names = ["desktop.py", "coven", "LICENSE", "LICENSE-COVEN-HOST", "platform.json"]
    if not present(PORTABLE):
        with tempfile.TemporaryDirectory(dir=PORTABLE.parent) as temp:
            staged = Path(temp) / "release"
            staged.mkdir()
            for name in names:
                shutil.copy2(HERE / name, staged / name)
            staged.rename(PORTABLE)
    require(not PORTABLE.is_symlink() and all(regular(PORTABLE / n) and
            (PORTABLE / n).read_bytes() == (HERE / n).read_bytes() for n in names),
            "APP", "install the user application", "matching portable release", "existing application differs",
            "Keep the existing application for review and contact support with this report.")
    return PORTABLE


def fixed_file(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        with path.open("x") as dst:
            dst.write(data)
    except FileExistsError:
        require(regular(path) and path.read_text() == data,
                "ENTRY", "register the application", "owned service or menu entry", "existing entry differs",
                "Ask your Linux administrator to inspect the existing Coven Host service or menu entry.")


def quoted(value, desktop=False):
    raw = str(value)
    require(all(ord(c) >= 32 for c in raw), "HOME", "register the application", "ordinary home directory",
            "unsupported control character", "Use a Linux account with a standard home directory.")
    raw = raw.replace("%", "%%").replace("\\", "\\\\").replace('"', '\\"')
    if desktop:
        # Desktop entries unescape the value once before tokenizing Exec.
        return '"' + raw.replace("`", "\\`").replace("$", "\\$").replace("\\", "\\\\") + '"'
    return '"' + raw.replace("$", "$$") + '"'


def register(app, backend):
    py = quoted(app / "desktop.py")
    unit = (f"[Unit]\nDescription=Coven Host local peer and folder sync\n\n[Service]\nType=notify\n"
            f"ExecStart=/usr/bin/env python3 -I {py} service\nUMask=0077\nRestart=on-failure\n"
            "RestartPreventExitStatus=78\nRestartSec=4\nTimeoutStartSec=100\nStandardOutput=null\nStandardError=null\n"
            "\n[Install]\nWantedBy=default.target\n")
    config = Path(os.environ.get("XDG_CONFIG_HOME") or HOME / ".config")
    unit_path = config / "systemd/user" / UNIT
    if backend == "systemd":
        fixed_file(unit_path, unit)
    else:
        fixed_file(config / "autostart/org.opencoven.CovenHost.desktop",
                   "[Desktop Entry]\nType=Application\nName=Coven Host sync\nTerminal=false\n"
                   f"Exec=/usr/bin/env python3 -I {quoted(app / 'desktop.py', True)} session\n")
    if app == PORTABLE:
        data = Path(os.environ.get("XDG_DATA_HOME") or HOME / ".local/share")
        cmd = "/usr/bin/env python3 -I " + quoted(app / "desktop.py", True) + " launch"
        fixed_file(data / "applications/org.opencoven.CovenHost.desktop",
                   "[Desktop Entry]\nType=Application\nName=Coven Host\nIcon=folder\nTerminal=false\nCategories=Utility;FileTools;\n"
                   f"Exec={cmd}\nActions=Diagnostics;\n\n[Desktop Action Diagnostics]\nName=Show diagnostics\nExec={cmd} --diagnostics\n")


def controlled(*args, timeout=100):
    return subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          stdin=subprocess.DEVNULL, timeout=timeout)


def has_manager():
    return bool(shutil.which("systemctl") and controlled("systemctl", "--user", "show-environment", timeout=5).returncode == 0)


def manager():
    require(has_manager(),
            "SESSION", "start the background service", "a running systemd user session", "user manager unavailable",
            "Sign in to a full Linux user session and open Coven Host. On a server, ask your administrator to enable the systemd user manager.")


def success(result, *fields):
    if result.returncode:
        raise Problem(*fields, detail=f"process exit {result.returncode}")


def command(*args, timeout=40):
    return controlled(HERE / "coven", "host", *args, timeout=timeout)


def runtime_env():
    os.environ.update(COVEN_HOME=str(STATE / "profile"), COVEN_HOST_STATE=str(STATE / "client"))


def check_legacy():
    candidates = [HOME / ".local/state/coven-host/client/client.sqlite", HOME / ".coven/host/client.sqlite",
                  HOME / ".local/state/coven-host-mvp/client/client.sqlite"]
    if os.environ.get("COVEN_HOME"):
        candidates.append(Path(os.environ["COVEN_HOME"]) / "host/client.sqlite")
    if os.environ.get("COVEN_HOST_STATE"):
        candidates.append(Path(os.environ["COVEN_HOST_STATE"]) / "client.sqlite")
    require(not any(present(p) and p != DB for p in candidates), "EXISTING", "protect the existing setup",
            "an unused Host profile", "existing Host client state",
            "Keep using your existing setup. Contact support before migrating it to the desktop installer.")


def prepare():
    receipt = STATE / "desktop.json"
    if not present(STATE):
        check_legacy()
        require(not present(ROOT), "FOLDER", "create the Coven folder", "an unused Coven folder name",
                "existing file, folder or link", "Back up the existing Coven item and move it to a different name if you want a new setup, then reopen Coven Host.")
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        STATE.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=STATE.parent) as temp:
            staged = Path(temp) / "state"
            staged.mkdir(mode=0o700)
            write_file(staged / "access-token", (secrets.token_urlsafe(48) + "\n").encode("ascii"))
            (staged / "desktop.json").write_text(json.dumps({"owner": "Coven Host Desktop", "port": port}))
            staged.rename(STATE)
    private(STATE)
    require(regular(receipt), "STATE", "protect the existing setup", "installer-owned state",
            "existing state without installation record", DB_HELP)
    record = json.loads(receipt.read_text())
    require(record.get("owner") == "Coven Host Desktop" and type(record.get("port")) is int and 1024 <= record["port"] <= 65535,
            "STATE", "inspect installation", "valid installation record", "invalid installation record", DB_HELP)
    require(regular(TOKEN) and TOKEN.stat().st_uid == os.getuid() and TOKEN.stat().st_mode & 0o077 == 0,
            "TOKEN", "check protected access", "original owner-only credential", "credential unavailable or unsafe", DB_HELP)
    if "root" not in record:
        require(not present(ROOT), "FOLDER", "reserve the Coven folder", "an unused folder name", "existing item",
                "Keep the existing Coven item and contact support before continuing.")
        ROOT.mkdir(mode=0o700)
        record["root"] = [ROOT.stat().st_dev, ROOT.stat().st_ino]
        write_file(receipt, json.dumps(record).encode())
    require(not ROOT.is_symlink() and ROOT.is_dir() and record["root"] == [ROOT.stat().st_dev, ROOT.stat().st_ino],
            "ROOT", "check the synchronized folder", "the original Coven folder", "folder missing, replaced or moved",
            "Restore the original Coven folder to its previous location and reopen the app.")
    url = "http://127.0.0.1:" + str(record["port"])
    require(not record.get("ready") or (regular(DB) and regular(STATE / "server/host.sqlite")),
            "DATABASE", "check saved state", "original peer and client databases", "a database is missing", DB_HELP)
    if present(DB):
        require(regular(DB), "DATABASE", "check saved state", "original client database", "unsafe database entry", DB_HELP)
        with sqlite3.connect(DB.as_uri() + "?mode=ro", uri=True) as db:
            cfg = dict(db.execute("SELECT key,value FROM settings WHERE key IN ('root','url','token_path')"))
        require(cfg.get("root") == str(ROOT) and cfg.get("url", url) == url and cfg.get("token_path", str(TOKEN)) == str(TOKEN)
                and (not record.get("ready") or cfg.get("url") == url and cfg.get("token_path") == str(TOKEN)),
                "REMOTE", "check the existing connection", "the original local peer and folder",
                "connection or folder settings differ", "Keep your existing connection. Contact support before changing peer or folder settings.")
    return record, url


def owns_listener(proc, port):
    # Check the peer owns the listening socket before the daemon can submit a credential.
    sockets = set()
    for fd in Path("/proc", str(proc.pid), "fd").glob("*"):
        try:
            sockets.add(os.readlink(fd))
        except FileNotFoundError:
            continue
    with open("/proc/net/tcp") as table:
        for line in table:
            row = line.split()
            if len(row) > 9 and row[1] == f"0100007F:{port:04X}" and row[3] == "0A" and "socket:[" + row[9] + "]" in sockets:
                return True
    return False


def start_child(*args):
    return subprocess.Popen([HERE / "coven", "host", *args], stdin=subprocess.DEVNULL,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def sync_problem(raw, returncode):
    try:
        obj = json.loads(raw)
    except ValueError:
        obj = {}
    detail = f"native sync exit {returncode}"
    if obj.get("phase") == "offline":
        return Problem("OFFLINE", "reach the local peer", "peer available", "peer offline",
                       "Leave your files in place and reopen Coven Host to retry. You can keep editing offline.", detail)
    if obj.get("conflicts") or obj.get("error") == "unresolved conflicts; see status and resolve":
        return Problem("CONFLICT", "synchronize files", "matching versions", "conflicting versions",
                       "Keep both versions. Use the operator guide to inspect and resolve the conflict, or send support this report.", detail)
    return Problem("SYNC", "synchronize files", "a synchronized folder", "sync needs attention",
                   "Keep your files in place and reopen the app. If the report returns, contact support.", detail)


def notify_ready():
    target = os.environ["NOTIFY_SOCKET"]
    with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as sock:
        sock.sendto(b"READY=1", target.replace("@", "\0", 1) if target.startswith("@") else target)


def service(systemd):
    record, url = prepare()
    runtime_env()
    peer = start_child("serve", "--port", str(record["port"]), "--storage", STATE / "server", "--token-file", TOKEN)
    children = [peer]
    try:
        for _ in range(150):
            if peer.poll() is not None or owns_listener(peer, record["port"]):
                break
            time.sleep(.1)
        require(peer.poll() is None and owns_listener(peer, record["port"]), "PEER", "start the private peer",
                "the bundled peer on its saved loopback port", "peer failed to listen",
                "Close any program using the saved peer port, then reopen Coven Host. If this repeats, send support this report.",
                f"peer exit {peer.returncode}" if peer.returncode is not None else "peer readiness timed out")
        if not present(DB) or not present(ROOT / ".coven-profile"):
            success(command("init", "--root", ROOT), "INIT", "initialize the folder",
                    "an initialized folder", "folder setup failed", RETRY)
        if not record.get("ready"):
            status = command("status")
            success(status, "STATUS", "inspect client configuration", "readable client state", "client unavailable", RETRY)
            if json.loads(status.stdout).get("remote") is None:
                success(command("connect", url, "--token-file", TOKEN), "CONNECT", "connect the local peer",
                        "a configured connection", "connection setup failed", RETRY)
            result = command("sync")
            if result.returncode:
                raise sync_problem(result.stdout, result.returncode)
            record["ready"] = True
            write_file(STATE / "desktop.json", json.dumps(record).encode())
        watch = start_child("watch")
        children.append(watch)
        if systemd:
            notify_ready()
        else:
            write_file(STATE / "session-ready", str(os.getpid()).encode())
        while peer.poll() is None and watch.poll() is None:
            time.sleep(.5)
        raise RuntimeError("service stopped")
    finally:
        for proc in children:
            if proc.poll() is None:
                proc.terminate()
        for proc in children:
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()


def supervise(systemd):
    verify(HERE)
    private(STATE)
    with (STATE / "service.lock").open("a") as lock:
        try:
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            if not systemd and sys.stdout.isatty():
                print("The Coven Host supervisor is already running.")
            require(not systemd, "SERVICE", "start the supervisor", "one active supervisor", "another supervisor is active", RETRY)
            return 0
        signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
        ready = STATE / "session-ready"
        while True:
            if ready.exists():
                ready.unlink()
            try:
                service(systemd)
            except RuntimeError:
                if systemd:
                    raise
                time.sleep(3)
            finally:
                if ready.exists():
                    ready.unlink()


def session_ready():
    # Stale PIDs alone never establish readiness. The supervisor also holds an exclusive lock.
    flag = STATE / "session-ready"
    if not regular(flag):
        return False
    with (STATE / "service.lock").open("a") as lock:
        try:
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            try:
                os.kill(int(flag.read_text()), 0)
                return True
            except (ValueError, OSError):
                return False
    return False


def start_session(app):
    if session_ready():
        return True
    proc = subprocess.Popen([sys.executable, "-I", app / "desktop.py", "session"], stdin=subprocess.DEVNULL,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True, cwd="/")
    deadline = time.monotonic() + 95
    while time.monotonic() < deadline:
        if session_ready():
            return True
        if proc.poll() is not None:
            return False
        time.sleep(.2)
    return False


def launch(portable, diagnostics, foreground):
    verify(HERE)
    private(SUPPORT)
    # The same lock serializes portable and native launches; systemd owns service startup.
    with (SUPPORT / "launch.lock").open("a") as lock:
        fcntl.flock(lock.fileno(), fcntl.LOCK_EX)
        # A new package type can reuse an existing service when its payload matches this release.
        app = copy_portable() if portable else HERE
        config = Path(os.environ.get("XDG_CONFIG_HOME") or HOME / ".config")
        unit = config / "systemd/user" / UNIT
        if regular(unit):
            for candidate in (SYSTEM, PORTABLE):
                if candidate != app and candidate.is_dir() and quoted(candidate / "desktop.py") in unit.read_text():
                    verify(candidate)
                    app = candidate
                    break
        prepare()
        backend_file = STATE / "startup"
        if regular(backend_file):
            backend = backend_file.read_text()
        else:
            backend = "systemd" if has_manager() else "session"
            fixed_file(backend_file, backend)
        require(backend in ("systemd", "session"), "STARTUP", "read startup settings", "known startup method", "unknown startup method", DB_HELP)
        register(app, backend)
        runtime_env()
        if backend == "systemd":
            manager()
            success(controlled("systemctl", "--user", "daemon-reload"), "REGISTER", "load the user service",
                    "registered user service", "systemd could not reload", RETRY)
        elif foreground:
            fcntl.flock(lock.fileno(), fcntl.LOCK_UN)
            print("Coven Host will synchronize while this process runs. Press Ctrl+C to stop.", flush=True)
            return supervise(False)
        # The service writes a redacted error before exiting; only use a report from this attempt.
        start = time.time_ns()
        start_exit = None
        if backend == "systemd":
            start_exit = controlled("systemctl", "--user", "start", UNIT, timeout=105).returncode
            started = start_exit == 0
        else:
            started = start_session(app)
        if not started:
            path = SUPPORT / "last-error.txt"
            if regular(path) and path.stat().st_mtime_ns >= start:
                display(path.read_text(), path)
                return 1
            raise Problem("START", "start the user service", "a running peer and watcher", "supervisor startup failed", RETRY,
                          f"systemd start exit {start_exit}" if start_exit is not None else None)
        if backend == "systemd":
            success(controlled("systemctl", "--user", "enable", UNIT), "LOGIN", "save login startup",
                    "service enabled for future logins", "systemd could not enable service", RETRY)
        result = command("sync")
        if result.returncode:
            raise sync_problem(result.stdout, result.returncode)
    if diagnostics:
        info = report(Problem("OK", "check folder and peer", "a synchronized folder", "synchronized", "Keep using your Coven folder."))
        if has_display():
            with tempfile.NamedTemporaryFile(prefix="coven-host-status-", suffix=".txt") as temp:
                Path(temp.name).write_text(info)
                display(info, Path(temp.name))
        else:
            print(info)
    elif has_display():
        require(open_with_desktop(ROOT), "OPEN", "open the folder", "a desktop file manager", "folder opener unavailable",
                "Open Coven in your home folder. Install or configure a file manager to open it from the applications menu.")
    else:
        print("Coven Host is ready. Open ~/Coven. Files in Castles/demo/files and results synchronize while your user service runs.")
        if backend == "session":
            print("Desktop login starts sync automatically. On a headless system, reopen Coven Host after each restart or run its launcher with --foreground from your service manager.")
    return 0


def main():
    os.umask(0o077)
    mode = sys.argv[1] if len(sys.argv) > 1 else "launch"
    try:
        require(os.getuid() != 0, "USER", "select user account", "your normal Linux account", "root account",
                "Open Coven Host from your own applications menu or terminal without sudo.")
        if mode in ("service", "session"):
            return supervise(mode == "service")
        return launch(mode == "portable", "--diagnostics" in sys.argv[2:], "--foreground" in sys.argv[2:])
    except Exception as err:
        if isinstance(err, Problem):
            problem = err
        elif isinstance(err, subprocess.TimeoutExpired):
            problem = Problem("TIMEOUT", "wait for setup", "timely response", "operation timed out", RETRY)
        elif isinstance(err, OSError):
            kind = errno.errorcode.get(err.errno, "OS error")
            # A source location is reproducible across installations and contains no profile or file names.
            frames = [f for f in traceback.extract_tb(err.__traceback__) if f.filename == __file__]
            location = " in desktop.py " + " > ".join(f"{frame.name}:{frame.lineno}" for frame in frames[-4:]) if frames else ""
            problem = Problem("STORAGE", "access local resources", "readable and writable local resources", kind,
                              "Check free disk space and user permissions, then reopen Coven Host.", f"errno {err.errno}{location}")
        else:
            problem = Problem("RUNTIME", "run setup", "valid installation and runtime", type(err).__name__, RETRY)
        body = report(problem)
        path = save_report(body)
        if mode not in ("service", "session"):
            display(body, path)
        return 1 if mode != "service" or not isinstance(err, Problem) else 78


if __name__ == "__main__":
    sys.exit(main())
