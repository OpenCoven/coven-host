import hmac
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import re
from urllib.parse import unquote
from uuid import uuid4

from .common import CASTLE, MAX_FILE, MAX_FILES, MAX_TOTAL, Problem, database, digest, validate_path


PREFIX = f"/v1/castles/{CASTLE}/files"


def initialize(path):
    with database(path) as db:
        db.executescript("""
            CREATE TABLE IF NOT EXISTS instance (slot INTEGER PRIMARY KEY, id TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS files (
                path TEXT PRIMARY KEY, revision INTEGER NOT NULL, sha256 TEXT NOT NULL,
                bytes BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS receipt (
                slot INTEGER PRIMARY KEY, operation TEXT NOT NULL, path TEXT NOT NULL,
                expected INTEGER NOT NULL, sha256 TEXT NOT NULL
            );
        """)
        db.execute("INSERT OR IGNORE INTO instance VALUES (1, ?)", (uuid4().hex,))
        return db.execute("SELECT id FROM instance").fetchone()[0]


class Handler(BaseHTTPRequestHandler):
    def setup(self):
        self.request.settimeout(5)
        super().setup()

    def log_message(self, *_):
        pass  # Never log authorization headers or request targets.

    def reply(self, status, data, revision=None):
        self.send_response(status)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Content-Type", "application/octet-stream" if revision else "application/json")
        self.send_header("Cache-Control", "no-store")
        if revision is not None:
            self.send_header("X-Revision", str(revision))
        self.end_headers()
        self.wfile.write(data)

    def error(self, status, message):
        self.reply(status, json.dumps({"error": message}).encode())

    def route(self, item_required=False):
        supplied = self.headers.get("Authorization", "").encode("latin-1")
        if not hmac.compare_digest(supplied, self.server.authorization):
            self.error(401, "credentials rejected; check the protected token file")
            return False, None
        pins = self.headers.get_all("X-Host-ID", [])
        if (pins and pins != [self.server.instance]) or (item_required and not pins):
            self.error(409, "host identity mismatch; check your tunnel and server storage")
            return False, None
        if self.path == PREFIX and not item_required:
            return True, None
        if not self.path.startswith(PREFIX + "/"):
            self.error(404, "unknown Castle or route")
            return False, None
        try:
            path = unquote(self.path[len(PREFIX) + 1:], errors="strict")
            validate_path(path)
        except (Problem, UnicodeError):
            self.error(400, "unsupported Castle path")
            return False, None
        # Reject aliases (encoded separators, percent escapes, query strings, etc.).
        if self.path != PREFIX + "/" + path:
            self.error(400, "noncanonical Castle path")
            return False, None
        return True, path

    def do_GET(self):
        allowed, path = self.route()
        if not allowed:
            return
        with database(self.server.db_path) as db:
            if path is None:
                rows = db.execute("SELECT path, revision, sha256, length(bytes) AS size FROM files ORDER BY path")
                data = json.dumps({"host": self.server.instance, "files": [dict(row) for row in rows]})
                self.reply(200, data.encode())
            else:
                row = db.execute("SELECT revision, bytes FROM files WHERE path=?", (path,)).fetchone()
                if row is None:
                    self.error(404, "file not found")
                else:
                    self.reply(200, row["bytes"], row["revision"])

    def do_PUT(self):
        allowed, path = self.route(item_required=True)
        if not allowed:
            return
        sizes = self.headers.get_all("Content-Length", [])
        expected = self.headers.get_all("If-Match", [])
        hashes = self.headers.get_all("X-Content-SHA256", [])
        operations = self.headers.get_all("X-Sync-Operation", [])
        if (self.headers.get("Transfer-Encoding") is not None
                or len(sizes) != 1 or not re.fullmatch(r"[0-9]{1,12}", sizes[0])
                or len(expected) != 1 or not re.fullmatch(r'"[0-9]{1,18}"', expected[0])
                or len(hashes) != 1 or not re.fullmatch(r"[a-f0-9]{64}", hashes[0])
                or len(operations) > 1
                or (operations and not re.fullmatch(r"[a-f0-9]{32}", operations[0]))):
            self.error(400, "invalid length, revision, hash or operation")
            return
        size, expected, sha = int(sizes[0]), int(expected[0][1:-1]), hashes[0]
        operation = operations[0] if operations else None
        if size > MAX_FILE:
            self.error(413, "file exceeds 10 MiB limit")
            return
        try:
            data = self.rfile.read(size)
        except TimeoutError:
            self.error(408, "upload incomplete")
            return
        if len(data) != size or digest(data) != sha:
            self.error(400, "upload incomplete or hash does not match")
            return
        with database(self.server.db_path) as db:
            db.execute("BEGIN IMMEDIATE")
            receipt = db.execute("SELECT operation, path, expected, sha256 FROM receipt").fetchone()
            if operation and receipt and receipt["operation"] == operation:
                if tuple(receipt) != (operation, path, expected, sha):
                    self.error(409, "operation ID already used for a different write")
                    return
            else:
                rows = db.execute("SELECT path, revision, length(bytes) FROM files").fetchall()
                current, previous_size = 0, 0
                for existing, revision, length in rows:
                    if existing == path:
                        current, previous_size = revision, length
                    elif existing.startswith(path + "/") or path.startswith(existing + "/"):
                        self.error(409, "file/directory name collision")
                        return
                if current != expected:
                    self.error(412, "remote revision changed; sync again")
                    return
                if ((not current and len(rows) >= MAX_FILES)
                        or sum(row[2] for row in rows) - previous_size + size > MAX_TOTAL):
                    self.error(413, "Castle exceeds 100 MiB or 1000 files")
                    return
                db.execute("INSERT OR REPLACE INTO files VALUES (?, ?, ?, ?)",
                           (path, expected + 1, sha, data))
                if operation:
                    db.execute("INSERT OR REPLACE INTO receipt VALUES (1, ?, ?, ?, ?)",
                               (operation, path, expected, sha))
        # The bytes, revision and retry receipt are committed before acknowledging.
        self.reply(200, json.dumps({"revision": expected + 1, "sha256": sha}).encode())


def serve(state, port, token):
    db_path = state / "host.sqlite"
    instance = initialize(db_path)
    with HTTPServer(("127.0.0.1", port), Handler) as server:
        server.db_path = db_path
        server.instance = instance
        server.authorization = b"Bearer " + token.encode("ascii")
        print(f"Serving Castle {CASTLE} on 127.0.0.1:{server.server_port}", flush=True)
        server.serve_forever()
