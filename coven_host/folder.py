import contextlib
import json
import os
from pathlib import Path
import stat
from uuid import uuid4

from .common import CASTLE, MAX_FILES, MAX_TOTAL, Problem, digest, dirfd, excluded, read_at, validate_path


MANIFEST = {"version": 1, "id": CASTLE, "displayName": "Demo Castle"}


class Folder:
    def __init__(self, root, state):
        self.root = root
        self.castle = root / "Castles" / "demo"
        self.state = state

    def initialize(self):
        for area in ("files", "results"):
            with dirfd(self.castle / area, create=True):
                pass
        with dirfd(self.castle) as fd:
            self.seed(fd, "castle.json", json.dumps(MANIFEST, indent=2) + "\n")
        with dirfd(self.root) as fd:
            self.seed(fd, "README.md", "# Coven\n\nOnly Castles/demo/files and results sync. "
                      "This prototype supports create and update, not deletion. "
                      "Run coven-host status for pending work and recovery paths.\n")
        self.check()

    @staticmethod
    def seed(fd, name, text):
        try:
            file = os.open(name, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600, dir_fd=fd)
        except FileExistsError:
            return
        with os.fdopen(file, "w", encoding="utf-8") as handle:
            handle.write(text)
            handle.flush()
            os.fsync(handle.fileno())
        os.fsync(fd)

    @contextlib.contextmanager
    def opened(self):
        with dirfd(self.castle) as fd:
            try:
                manifest = json.loads(read_at(fd, "castle.json", 4096))
            except (ValueError, RecursionError):
                raise Problem("invalid castle.json; recover the original demo manifest") from None
            if (not isinstance(manifest, dict) or type(manifest.get("version")) is not int
                    or manifest["version"] != 1 or manifest.get("id") != CASTLE):
                raise Problem("wrong Castle manifest; recover the original demo folder")
            yield fd

    def check(self):
        try:
            with self.opened() as fd:
                for area in ("files", "results"):
                    with dirfd(area, fd):
                        pass
        except OSError as exc:
            raise Problem(f"root unavailable or unsafe: {self.root} ({exc.strerror}); mount or restore it") from None

    def scan(self, sizes=None):
        self.check()
        hashes = {}
        total = count = 0

        def walk(fd, prefix):
            nonlocal total, count
            with os.scandir(fd) as entries:
                names = sorted(entry.name for entry in entries if not excluded(entry.name))
            for name in names:
                path = f"{prefix}/{name}"
                validate_path(path)
                count += 1
                if count > 2 * MAX_FILES:
                    raise Problem("Castle exceeds 2000 file and directory entries")
                if stat.S_ISDIR(os.stat(name, dir_fd=fd, follow_symlinks=False).st_mode):
                    with dirfd(name, fd) as child:
                        walk(child, path)
                else:
                    data = read_at(fd, name)
                    total += len(data)
                    if total > MAX_TOTAL or len(hashes) >= MAX_FILES:
                        raise Problem("local Castle exceeds 100 MiB or 1000 files")
                    hashes[path] = digest(data)
                    if sizes is not None:
                        sizes[path] = len(data)

        with self.opened() as fd:
            for area in ("files", "results"):
                with dirfd(area, fd) as child:
                    walk(child, area)
        return hashes

    def read(self, path):
        parts = validate_path(path)
        with self.opened() as fd:
            try:
                with dirfd("/".join(parts[:-1]), fd) as parent:
                    return read_at(parent, parts[-1])
            except FileNotFoundError:
                return None

    def replace(self, path, data, expected):
        self.check()
        parts = validate_path(path)
        relative = "/".join(parts[:-1])
        name = parts[-1]
        # Stage privately, on the same filesystem; never expose a partial document.
        with dirfd(self.state) as scratch, self.opened() as base:
            tmp = "stage-" + uuid4().hex
            try:
                with os.fdopen(os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600,
                                       dir_fd=scratch), "wb") as file:
                    file.write(data)
                    file.flush()
                    os.fsync(file.fileno())
                with dirfd(parts[0], base) as area:
                    # The scope directory must already exist; create only nested folders.
                    with dirfd("/".join(parts[1:-1]), area, create=True) as parent:
                        with self.opened() as live, dirfd(relative, live) as current:
                            a, b = os.fstat(parent), os.fstat(current)
                            if (a.st_dev, a.st_ino) != (b.st_dev, b.st_ino):
                                raise Problem(f"{path}: parent moved during transfer; retry")
                            try:
                                previous = digest(read_at(current, name))
                            except FileNotFoundError:
                                previous = None
                            if previous != expected:
                                raise Problem(f"{path}: local file changed during transfer; retry")
                            if a.st_dev != os.fstat(scratch).st_dev:
                                raise Problem("local root and client state must be on the same filesystem")
                            if expected is None:
                                # Creation must not overwrite a file created after the recheck.
                                os.link(tmp, name, src_dir_fd=scratch, dst_dir_fd=current)
                                os.unlink(tmp, dir_fd=scratch)
                            else:
                                os.replace(tmp, name, src_dir_fd=scratch, dst_dir_fd=current)
                            os.fsync(current)
                            os.fsync(scratch)
            finally:
                with contextlib.suppress(FileNotFoundError):
                    os.unlink(tmp, dir_fd=scratch)


def read_external(path):
    with dirfd(Path(path).parent) as fd:
        return read_at(fd, Path(path).name)
