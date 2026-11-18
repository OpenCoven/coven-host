import http.client
import json
import re
from urllib.parse import urlsplit

from .common import MAX_FILE, MAX_FILES, MAX_TOTAL, Problem, digest, validate_path
from .host import PREFIX


class Offline(Problem):
    pass


class Rejected(Problem):
    def __init__(self, status, message):
        super().__init__(f"host HTTP {status}: {message}")
        self.status = status


def endpoint(url):
    try:
        parts = urlsplit(url)
        if (parts.scheme in {"http", "https"} and parts.hostname and not parts.username
                and not parts.password and parts.path in ("", "/")
                and not parts.query and not parts.fragment and parts.port != 0
                and (parts.scheme == "https" or parts.hostname == "127.0.0.1")
                and re.fullmatch(r"https?://[A-Za-z0-9.:\[\]-]+/?", url)):
            return parts
    except ValueError:
        pass
    raise Problem("use verified https:// or http://127.0.0.1:<port> (local/verified SSH tunnel)")


def parse_json(raw):
    try:
        return json.loads(raw)
    except (ValueError, RecursionError):
        raise Problem("host returned invalid JSON") from None


class Remote:
    def __init__(self, url, token, identity=None):
        self.parts = endpoint(url)
        self.authorization = "Bearer " + token
        self.identity = identity

    def request(self, method, path=None, data=None, headers=None, limit=1024**2):
        route = PREFIX + ("/" + path if path else "")
        cls = http.client.HTTPSConnection if self.parts.scheme == "https" else http.client.HTTPConnection
        connection = cls(self.parts.hostname, self.parts.port, timeout=5)
        headers = dict(headers or {}, Authorization=self.authorization)
        if self.identity:
            headers["X-Host-ID"] = self.identity
        try:
            connection.request(method, route, body=data, headers=headers)
            with connection.getresponse() as response:
                body = response.read((limit if response.status == 200 else 4096) + 1)
                if response.status != 200:
                    error = {
                        400: "invalid request or unsupported path", 401: "check the token file",
                        404: "file or Castle not found; retry reconciliation",
                        409: "check host identity or operation ID",
                        412: "remote revision changed; sync again",
                        413: "upload exceeds Castle limits",
                        422: "file/directory collision; move the unsynchronized local entry",
                    }.get(response.status, "check server and tunnel")
                    raise Rejected(response.status, error)
                if len(body) > limit:
                    raise Problem("host response exceeds its size limit")
                return body, response.getheader("X-Revision")
        except (OSError, http.client.HTTPException) as exc:
            raise Offline(f"cannot contact host ({type(exc).__name__}); check tunnel, service or TLS") from None
        finally:
            connection.close()

    def inventory(self):
        payload = parse_json(self.request("GET")[0])
        if not isinstance(payload, dict):
            raise Problem("invalid host inventory")
        identity, rows = payload.get("host"), payload.get("files")
        if (not isinstance(identity, str) or not re.fullmatch(r"[a-f0-9]{32}", identity)
                or not isinstance(rows, list) or len(rows) > MAX_FILES):
            raise Problem("invalid host inventory")
        if self.identity and self.identity != identity:
            raise Problem("host identity changed; check the configured host and its persistent storage")
        result = {}
        total = 0
        for item in rows:
            if not isinstance(item, dict):
                raise Problem("invalid host file metadata")
            path, revision, sha, size = (item.get(k) for k in ("path", "revision", "sha256", "size"))
            if (not isinstance(path, str) or path in result or type(revision) is not int
                    or not 1 <= revision < 10**18 or not isinstance(sha, str)
                    or not re.fullmatch(r"[a-f0-9]{64}", sha)
                    or type(size) is not int or not 0 <= size <= MAX_FILE):
                raise Problem("invalid host file metadata")
            validate_path(path)
            result[path] = (revision, sha, size)
            total += size
        if total > MAX_TOTAL:
            raise Problem("host inventory exceeds Castle storage limit")
        self.identity = identity
        return result

    def download(self, path, item):
        revision, sha, size = item
        data, reported_revision = self.request("GET", path, limit=MAX_FILE)
        if (str(revision), sha, size) != (reported_revision, digest(data), len(data)):
            raise Problem(f"{path}: host file changed or transfer failed verification; sync again")
        return data

    def upload(self, path, data, expected, operation=None):
        validate_path(path)
        if len(data) > MAX_FILE:
            raise Problem(f"{path}: exceeds 10 MiB limit")
        sha = digest(data)
        headers = {"If-Match": f'"{expected}"', "X-Content-SHA256": sha}
        if operation:
            headers["X-Sync-Operation"] = operation
        response = parse_json(self.request("PUT", path, data, headers)[0])
        if response != {"revision": expected + 1, "sha256": sha}:
            raise Problem("invalid upload acknowledgement; retry reconciliation")
        return expected + 1, sha
