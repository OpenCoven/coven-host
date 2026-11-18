import contextlib
import hashlib
import os
from pathlib import Path
import re
import sqlite3
import stat


CASTLE = "demo"
MAX_FILE = 10 * 1024**2
MAX_TOTAL = 100 * 1024**2
MAX_FILES = 1000
DIR = os.O_DIRECTORY | os.O_NOFOLLOW
TOKEN_PATTERN = re.compile(r"[A-Za-z0-9_-]{43,128}", re.ASCII)


class Problem(Exception):
    pass


def digest(data):
    return hashlib.sha256(data).hexdigest()


def excluded(name):
    name = name.lower()
    return (
        name.startswith((".", "~", "credentials", "secrets", "token", "id_"))
        or name in {"node_modules", "__pycache__", "vendor", "target", "cache", "caches"}
        or name.endswith((".key", ".pem", ".p12", ".pfx", ".ppk", ".kdbx",
                          ".tmp", ".temp", ".swp", ".part", ".bak", ".sqlite", ".db", "~"))
    )


def validate_path(path):
    parts = path.split("/")
    if (len(path) > 512 or not 2 <= len(parts) <= 16
            or parts[0] not in {"files", "results"}
            or any(excluded(p) or not re.fullmatch(r"[a-z0-9_-][a-z0-9_.-]{0,119}", p)
                   or p.endswith(".")
                   or p.split(".")[0] in {
                       "con", "prn", "aux", "nul",
                       *(f"{prefix}{i}" for prefix in ("com", "lpt") for i in range(1, 10)),
                   } for p in parts[1:])):
        raise Problem(f"unsupported Castle path: {path!r}")
    return parts


@contextlib.contextmanager
def dirfd(path, start=None, create=False):
    """Walk one component at a time; never follow a directory symlink."""
    parts = Path(path).parts
    fd = os.open("/", DIR) if start is None else os.dup(start)
    try:
        for part in parts[1:] if start is None else parts:
            if part in {"/", ".", ".."}:
                raise Problem("unsafe directory component")
            if create:
                try:
                    os.mkdir(part, mode=0o700, dir_fd=fd)
                    os.fsync(fd)
                except FileExistsError:
                    pass
            child = os.open(part, DIR, dir_fd=fd)
            os.close(fd)
            fd = child
        yield fd
    finally:
        os.close(fd)


def read_at(fd, name, limit=MAX_FILE):
    entry = os.stat(name, dir_fd=fd, follow_symlinks=False)
    if not stat.S_ISREG(entry.st_mode) or entry.st_nlink != 1:
        raise Problem(f"{name!r}: expected a regular file (no symlinks or hard links)")
    with os.fdopen(os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK,
                           dir_fd=fd), "rb") as file:
        before = os.fstat(file.fileno())
        if (not stat.S_ISREG(before.st_mode) or before.st_nlink != 1
                or (before.st_dev, before.st_ino) != (entry.st_dev, entry.st_ino)):
            raise Problem(f"{name!r}: file changed while opening; retry")
        if before.st_size > limit:
            raise Problem(f"{name!r}: exceeds {limit} byte limit")
        data = file.read(limit + 1)
        after = os.stat(name, dir_fd=fd, follow_symlinks=False)
        def fingerprint(info):
            return (info.st_dev, info.st_ino, info.st_mode, info.st_nlink, info.st_size,
                    info.st_mtime_ns, info.st_ctime_ns)

        if fingerprint(before) != fingerprint(after):
            raise Problem(f"{name!r}: file changed while reading; retry")
        if len(data) > limit:
            raise Problem(f"{name!r}: exceeds {limit} byte limit")
        return data


def private_dir(path):
    with dirfd(path, create=True) as fd:
        info = os.fstat(fd)
        if info.st_uid != os.getuid() or info.st_mode & 0o077:
            raise Problem(f"make {path} private and owned by you (chmod 700)")


def token_from(path):
    with dirfd(path.parent) as fd:
        entry = os.stat(path.name, dir_fd=fd, follow_symlinks=False)
        if entry.st_uid != os.getuid() or entry.st_mode & 0o077:
            raise Problem("token file must be owned by you with mode 600 (or stricter)")
        try:
            token = read_at(fd, path.name, 256).decode("ascii").strip()
        except UnicodeError:
            raise Problem("invalid token file format") from None
    if not TOKEN_PATTERN.fullmatch(token):
        raise Problem("token file must contain one URL-safe token of 43–128 characters")
    return token


@contextlib.contextmanager
def database(path):
    db = sqlite3.connect(path, timeout=5)
    db.row_factory = sqlite3.Row
    try:
        db.execute("PRAGMA journal_mode=DELETE")
        db.execute("PRAGMA synchronous=FULL")
        with db:
            yield db
    finally:
        db.close()
