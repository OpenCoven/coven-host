"""Exercise macOS token ACLs through an installed CLI or the Rust compatibility adapter."""
import argparse
import ctypes
from contextlib import ExitStack
import os
from pathlib import Path
import stat
import subprocess
import tempfile
import uuid

from installed_deletion import stop, until, write_token
from test_multi_client import Client, KEY


def set_entry(path, entry):
    libc = ctypes.CDLL(None, use_errno=True)
    libc.acl_from_text.argtypes = [ctypes.c_char_p]
    libc.acl_from_text.restype = ctypes.c_void_p
    libc.acl_set_fd_np.argtypes = [ctypes.c_int, ctypes.c_void_p, ctypes.c_int]
    libc.acl_free.argtypes = [ctypes.c_void_p]
    acl = libc.acl_from_text(("!#acl 1\n" + entry + "\n").encode())
    if not acl:
        raise OSError(ctypes.get_errno(), "acl_from_text")
    try:
        with path.open("rb") as handle:
            if libc.acl_set_fd_np(handle.fileno(), acl, 0x100):
                raise OSError(ctypes.get_errno(), "acl_set_fd_np")
    finally:
        libc.acl_free(acl)
    assert stat.S_IMODE(path.stat().st_mode) == 0o600


def exercise(binary, adapter):
    with tempfile.TemporaryDirectory() as directory, ExitStack() as cleanup:
        work = Path(directory).resolve()
        token = work / "token"
        write_token(token)
        secret = token.read_bytes()
        client = Client(binary, work / "owner", adapter)
        prefix = [binary] if adapter else [binary, "host"]
        log = work / "peer.log"
        with log.open("wb") as out:
            peer = subprocess.Popen([*prefix, "serve", "--storage", work / "peer", "--port", "0",
                                     "--token-file", token], env=client.env, stdout=out)
        cleanup.callback(stop, peer)
        until(lambda: peer.poll() is not None or "Serving Castle" in log.read_text())
        assert peer.poll() is None
        url = "http://127.0.0.1:" + log.read_text().splitlines()[0].rsplit(":", 1)[1]
        cleanup.callback(client.disconnect)
        client.run("init", "--root", client.root)

        def connect(allowed, endpoint=url):
            dispatch = [binary, "--state", client.state] if adapter else prefix
            p = subprocess.run([*dispatch, "connect", endpoint, "--token-file", token],
                               env=client.env, capture_output=True, timeout=60)
            diagnostic = p.stdout + p.stderr
            assert secret not in diagnostic, "credential present in diagnostic"
            assert (p.returncode == 0) == allowed, "unexpected connect result"
            if not allowed:
                assert b"ACL" in diagnostic, "refusal did not identify ACL"

        connect(True)
        print("PASS private mode 0600")
        owner = "ABCDEFAB-CDEF-ABCD-EFAB-CDEF0000000A"
        synthetic = f"FFFFEEEE-DDDD-CCCC-BBBB-AAAA{os.geteuid():08X}"
        everyone = "ABCDEFAB-CDEF-ABCD-EFAB-CDEF0000000C"
        account = (ctypes.c_ubyte * 16)()
        assert ctypes.CDLL(None).mbr_uid_to_uuid(os.geteuid(), account) == 0
        for label, entry in [
            ("account owner", f"user:{uuid.UUID(bytes=bytes(account))}:::allow:read"),
            ("file owner", f"user:{owner}:::allow:read"),
            ("UID owner", f"user:{synthetic}:::allow:read"),
            ("deny", f"group:{everyone}:::deny:writeattr"),
        ]:
            set_entry(token, entry)
            connect(True)
            print("PASS", label)
        token = work / "replacement"
        token.write_bytes(secret)
        token.chmod(0o600)
        set_entry(token, f"group:{everyone}:::allow:read")
        connect(False, "http://127.0.0.1:1")
        client.file.write_bytes(b"connection survived refusal")
        client.run("sync")
        subprocess.run(["/bin/chmod", "-N", token], check=True)
        connect(True)
        assert client.run("get", KEY, "--url", url, "--token-file", token) == client.file.read_bytes()
        print("PASS non-owner read refused, existing connection synced, corrected file reconnected")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--adapter", action="store_true")
    args = parser.parse_args()
    exercise(args.binary.resolve(), args.adapter)
