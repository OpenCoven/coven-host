"""Loopback transport faults for peer integration tests."""
import http.client
from http.server import BaseHTTPRequestHandler


class FaultProxy(BaseHTTPRequestHandler):
    def relay(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        conn = http.client.HTTPConnection("127.0.0.1", self.server.upstream, timeout=5)
        try:
            conn.request(self.command, self.path, body, dict(self.headers))
            with conn.getresponse() as response:
                data = response.read()
                if (self.command, self.server.fault) in {("PUT", "ack"), ("DELETE", "delete-ack")}:
                    self.server.fault = None
                    return  # Drop the response after the upstream commit.
                self.send_response(response.status)
                self.send_header("Content-Length", str(len(data)))
                if response.getheader("X-Revision"):
                    self.send_header("X-Revision", response.getheader("X-Revision"))
                    if self.server.fault == "download":
                        self.server.fault = None
                        data = data[:1]
                self.end_headers()
                self.wfile.write(data)
        finally:
            conn.close()

    do_GET = do_PUT = do_DELETE = relay

    def log_message(self, *_):
        pass
