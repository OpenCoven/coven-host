import contextlib
from datetime import datetime, timezone
import fcntl
import os
from pathlib import Path
import re
from uuid import uuid4

from .common import MAX_FILES, MAX_TOTAL, Problem, database, digest, dirfd, read_at, token_from, validate_path
from .folder import Folder
from .remote import Offline, Rejected, Remote, endpoint


@contextlib.contextmanager
def locked(state, required=True):
    with (state / "client.lock").open("a+b") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            if required:
                raise Problem("sync is running; stop the foreground process first") from None
            yield False
        else:
            yield True


class Client:
    def __init__(self, state):
        self.state = state
        self.path = state / "client.sqlite"

    @contextlib.contextmanager
    def db(self):
        with database(self.path) as db:
            yield db

    def initialize(self, root):
        with self.db() as db:
            db.executescript("""
                CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                CREATE TABLE IF NOT EXISTS baseline (
                    path TEXT PRIMARY KEY, revision INTEGER NOT NULL, sha TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS pending (
                    slot INTEGER PRIMARY KEY, path TEXT NOT NULL, expected INTEGER NOT NULL,
                    operation TEXT NOT NULL, bytes BLOB NOT NULL
                );
                CREATE TABLE IF NOT EXISTS conflicts (
                    path TEXT PRIMARY KEY, local TEXT NOT NULL, remote TEXT NOT NULL,
                    revision INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS incoming (
                    slot INTEGER PRIMARY KEY, path TEXT NOT NULL, revision INTEGER NOT NULL, sha TEXT NOT NULL
                );
            """)
            stored = self.setting(db, "root")
            if stored and stored != str(root):
                raise Problem("client state is already bound to another root; use a different --state")
            Folder(root, self.state).initialize()
            self.set(db, "root", str(root))

    @staticmethod
    def setting(db, key, default=None):
        row = db.execute("SELECT value FROM settings WHERE key=?", (key,)).fetchone()
        return row[0] if row else default

    @staticmethod
    def set(db, key, value):
        db.execute("INSERT OR REPLACE INTO settings VALUES (?, ?)", (key, value))

    def config(self, url, token_path):
        endpoint(url)
        token_from(token_path)
        with self.db() as db:
            self.set(db, "url", url)
            self.set(db, "token_path", str(token_path))
            self.set(db, "phase", "needs attention")
            self.set(db, "error", "connection configured; run sync to reconcile")

    def remote(self):
        with self.db() as db:
            url, token, identity = (self.setting(db, key) for key in ("url", "token_path", "host"))
        if not url:
            raise Problem("no remote configured; run connect")
        # Config stores only the protected file's path, never the token.
        return Remote(url, token_from(Path(token)), identity)

    def inventory(self, remote):
        result = remote.inventory()
        with self.db() as db:
            self.set(db, "host", remote.identity)
            self.set(db, "contact", datetime.now(timezone.utc).isoformat(timespec="seconds"))
        return result

    def phase(self, phase, error=""):
        with self.db() as db:
            self.set(db, "phase", phase)
            self.set(db, "error", error)

    def baseline(self):
        with self.db() as db:
            return {row["path"]: (row["revision"], row["sha"])
                    for row in db.execute("SELECT * FROM baseline")}

    @staticmethod
    def record(db, path, item):
        db.execute("INSERT OR REPLACE INTO baseline VALUES (?, ?, ?)", (path, *item[:2]))

    def flush_pending(self, remote):
        with self.db() as db:
            pending = db.execute("SELECT path, expected, operation, bytes FROM pending").fetchone()
        if pending is None:
            return
        path, expected, operation, data = pending
        try:
            item = remote.upload(path, data, expected, operation)
        except Rejected as exc:
            # Definitive rejection. A lost or invalid acknowledgement leaves the outbox intact.
            if exc.status in {412, 413}:
                with self.db() as db:
                    db.execute("DELETE FROM pending")
                if exc.status == 412:
                    return
            raise
        with self.db() as db:
            self.record(db, path, item)
            db.execute("DELETE FROM pending")

    def archive(self, path, side, data):
        # Flat, content-addressed names bound retries and keep the original path out of OS lookups.
        name = f"{digest(path.encode())[:24]}-{side}-{digest(data)}"
        with dirfd(self.state / "recovery", create=True) as fd:
            try:
                if read_at(fd, name) != data:
                    raise Problem("recovery copy failed verification")
            except FileNotFoundError:
                with os.scandir(fd) as entries:
                    sizes = [entry.stat(follow_symlinks=False).st_size for entry in entries]
                if len(sizes) >= 2000 or sum(sizes) + len(data) > MAX_TOTAL:
                    raise Problem("recovery is full; back up and move resolved copies out of recovery")
                tmp = ".save-" + uuid4().hex
                try:
                    with os.fdopen(os.open(tmp, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600,
                                           dir_fd=fd), "wb") as file:
                        file.write(data)
                        file.flush()
                        os.fsync(file.fileno())
                    os.rename(tmp, name, src_dir_fd=fd, dst_dir_fd=fd)
                    os.fsync(fd)
                finally:
                    with contextlib.suppress(FileNotFoundError):
                        os.unlink(tmp, dir_fd=fd)
        return name

    def conflict(self, path, folder, remote, sha, item):
        local = folder.read(path)
        if local is None or digest(local) != sha:
            raise Problem(f"{path}: local file changed during reconciliation; sync again")
        distant = remote.download(path, item)
        local_name = self.archive(path, "local", local)
        remote_name = self.archive(path, "remote", distant)
        with self.db() as db:
            db.execute("INSERT OR REPLACE INTO conflicts VALUES (?, ?, ?, ?)",
                       (path, local_name, remote_name, item[0]))

    def recover_incoming(self, local):
        with self.db() as db:
            pending = db.execute("SELECT path, revision, sha FROM incoming").fetchone()
            if pending:
                path, revision, sha = pending
                if local.get(path) == sha:
                    self.record(db, path, (revision, sha))
                db.execute("DELETE FROM incoming")

    def clean_stages(self):
        for directory, pattern in ((self.state, r"stage-[a-f0-9]{32}"),
                                   (self.state / "recovery", r"\.save-[a-f0-9]{32}")):
            try:
                with dirfd(directory) as fd:
                    for name in os.listdir(fd):
                        if re.fullmatch(pattern, name):
                            os.unlink(name, dir_fd=fd)
                    os.fsync(fd)
            except FileNotFoundError:
                pass

    def reconcile(self, folder):
        self.clean_stages()
        sizes = {}
        local = folder.scan(sizes)
        self.recover_incoming(local)
        remote = self.remote()
        self.inventory(remote)
        self.flush_pending(remote)
        items = self.inventory(remote)
        base = self.baseline()
        with self.db() as db:
            conflicts = {row["path"]: row for row in db.execute("SELECT * FROM conflicts")}
        errors = []
        for path in sorted(local.keys() | items.keys() | base.keys()):
            sha, item = local.get(path), items.get(path)
            old_revision, old_sha = base.get(path, (0, None))
            if old_revision and item is None:
                errors.append(f"{path}: remote baseline missing; check persistent host storage")
                continue
            if sha is None and old_revision:
                errors.append(f"{path}: deletion propagation unsupported; remote copy preserved")
                continue
            if path in conflicts:
                row = conflicts[path]
                if item and sha and (not row["local"].endswith("-" + sha)
                                     or not row["remote"].endswith("-" + item[1])
                                     or row["revision"] != item[0]):
                    self.conflict(path, folder, remote, sha, item)
                continue  # An existing conflict is never silently resolved.
            if item and sha == item[1]:
                if (old_revision, old_sha) != item[:2]:
                    with self.db() as db:
                        self.record(db, path, item)
            elif item and item[1] != old_sha and sha != old_sha and sha is not None:
                self.conflict(path, folder, remote, sha, item)
            elif item and sha == old_sha:
                if ((path not in sizes and len(sizes) >= MAX_FILES)
                        or sum(sizes.values()) - sizes.get(path, 0) + item[2] > MAX_TOTAL):
                    errors.append(f"{path}: download would exceed local Castle limits")
                    continue
                data = remote.download(path, item)
                with self.db() as db:
                    db.execute("INSERT INTO incoming VALUES (1, ?, ?, ?)", (path, *item[:2]))
                folder.replace(path, data, sha)
                with self.db() as db:
                    self.record(db, path, item)
                    db.execute("DELETE FROM incoming")
                sizes[path] = len(data)
            elif sha is not None:
                data = folder.read(path)
                if data is None or digest(data) != sha:
                    raise Problem(f"{path}: local file changed during reconciliation; sync again")
                # When a revision changed but its bytes did not, use the observed revision.
                expected = item[0] if item else 0
                with self.db() as db:
                    db.execute("INSERT INTO pending VALUES (1, ?, ?, ?, ?)",
                               (path, expected, uuid4().hex, data))
                self.flush_pending(remote)

        with self.db() as db:
            if db.execute("SELECT 1 FROM conflicts").fetchone():
                errors.append("unresolved conflicts; see status and resolve")
        if not errors:
            expected = self.baseline()
            final_remote = {path: item[:2] for path, item in self.inventory(remote).items()}
            if (final_remote != expected
                    or folder.scan() != {path: sha for path, (_, sha) in expected.items()}):
                errors.append("files changed during reconciliation; sync again")
        return "; ".join(errors)

    def sync(self, folder):
        self.phase("syncing")
        try:
            error = self.reconcile(folder)
            phase = "needs attention" if error else "synced"
        except Offline as exc:
            phase, error = "offline", str(exc)
        except (Problem, OSError) as exc:
            phase, error = "needs attention", str(exc)
        self.phase(phase, error)
        return phase, error

    def resolve(self, folder, path, choice, merged):
        validate_path(path)
        with self.db() as db:
            row = db.execute("SELECT * FROM conflicts WHERE path=?", (path,)).fetchone()
        if row is None:
            raise Problem(f"{path}: no recorded conflict")
        self.clean_stages()
        sizes = {}
        folder.scan(sizes)
        remote = self.remote()
        item = self.inventory(remote).get(path)
        local = folder.read(path)
        if (item is None or local is None or item[0] != row["revision"]
                or not row["local"].endswith("-" + digest(local))
                or not row["remote"].endswith("-" + item[1])):
            raise Problem("conflict inputs changed; restore a missing file or sync again before resolving")
        remote_data = remote.download(path, item)
        self.archive(path, "local", local)
        self.archive(path, "remote", remote_data)
        chosen = {"local": local, "remote": remote_data, "merged": merged}[choice]
        if chosen != local:
            if sum(sizes.values()) - len(local) + len(chosen) > MAX_TOTAL:
                raise Problem("resolution exceeds Castle storage limits")
            folder.replace(path, chosen, digest(local))
        with self.db() as db:
            self.record(db, path, item)
            db.execute("DELETE FROM conflicts WHERE path=?", (path,))
            self.set(db, "phase", "needs attention")
            self.set(db, "error", "resolution saved; run sync to confirm convergence")

    def status(self, folder, active):
        with self.db() as db:
            phase = self.setting(db, "phase", "needs attention")
            error = self.setting(db, "error", "run connect and sync")
            url, contact = (self.setting(db, key, "(none)") for key in ("url", "contact"))
            conflicts = [dict(row) for row in db.execute("SELECT * FROM conflicts ORDER BY path")]
            pending = db.execute("SELECT path FROM pending").fetchone()
        base = self.baseline()
        pending_paths = set()
        try:
            local = folder.scan()
            pending_paths = {path for path in local.keys() | base.keys()
                             if local.get(path) != base.get(path, (None, None))[1]}
            deleted = base.keys() - local.keys()
            if deleted:
                error = "deletion propagation unsupported; remote copy preserved: " + ", ".join(sorted(deleted))
        except (Problem, OSError) as exc:
            pending_paths = None
            phase, error = "needs attention", str(exc)
        if active:
            phase = "syncing"
        elif phase == "syncing":
            phase, error = "needs attention", "sync stopped before reconciliation; run sync"
        elif phase == "synced" and (pending_paths or pending or conflicts):
            phase = "needs attention"
            error = error or "local work pending; run sync"
        print(f"Status: {phase}\nRoot: {folder.root}\nRemote: {url}\nLast successful contact: {contact}")
        count = "unknown" if pending_paths is None else str(len(pending_paths | ({pending[0]} if pending else set())))
        print(f"Pending local paths: {count}\nConflicts: {len(conflicts)}\nRecovery: {self.state / 'recovery'}")
        if pending:
            print(f"Awaiting acknowledgement: {pending[0]}")
        for conflict in conflicts:
            print(f"  {conflict['path']}\n    local: {self.state / 'recovery' / conflict['local']}"
                  f"\n    remote: {self.state / 'recovery' / conflict['remote']}")
        if error:
            print(f"Action: {error}")
