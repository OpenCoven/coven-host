import argparse
import os
from pathlib import Path
import sqlite3
import sys
import time

from .client import Client, locked
from .common import CASTLE, Problem, private_dir, token_from, validate_path
from .folder import Folder, read_external
from .host import serve
from .remote import Remote


def absolute(path):
    return Path(os.path.abspath(os.path.expanduser(path)))


def parser():
    cli = argparse.ArgumentParser(prog="coven-host")
    cli.add_argument("--state", default="~/.local/state/coven-host/client", help="private client state directory")
    commands = cli.add_subparsers(dest="command", required=True)
    init = commands.add_parser("init", help="initialize an ordinary Coven directory")
    init.add_argument("--root", default="~/Coven")
    connect = commands.add_parser("connect", help="configure a verified HTTPS URL or loopback tunnel")
    connect.add_argument("url")
    connect.add_argument("--token-file", required=True)
    commands.add_parser("sync", help="reconcile once")
    commands.add_parser("watch", help="reconcile in the foreground until Ctrl-C")
    commands.add_parser("status", help="show local work and the last reconciliation outcome")
    resolve = commands.add_parser("resolve", help="resolve a conflict; retained copies remain in recovery")
    resolve.add_argument("path")
    choice = resolve.add_mutually_exclusive_group(required=True)
    choice.add_argument("--local", action="store_true")
    choice.add_argument("--remote", action="store_true")
    choice.add_argument("--merged", metavar="FILE")
    server = commands.add_parser("serve", help="serve one Castle on IPv4 loopback")
    server.add_argument("--storage", default="~/.local/state/coven-host/server")
    server.add_argument("--port", type=int, default=8765)
    server.add_argument("--token-file", required=True)
    for name in ("demo-result", "put", "get"):
        writer = commands.add_parser(name, help={
            "demo-result": "publish the deterministic demo artifact (not an agent)",
            "put": "publish a file via the authenticated API",
            "get": "read a file via the authenticated API to stdout",
        }[name])
        writer.add_argument("--url", default="http://127.0.0.1:8765")
        writer.add_argument("--token-file", required=True)
        if name != "demo-result":
            writer.add_argument("path")
        if name == "put":
            writer.add_argument("source")
    return cli


def separate(root, state):
    if (state.resolve().is_relative_to(root.resolve())
            or root.resolve().is_relative_to(state.resolve())):
        raise Problem("client state and the Coven root must be separate, non-nested directories")


def execute(args):
    command = args.command
    if command == "serve":
        if not 0 <= args.port <= 65535:
            raise Problem("invalid service port")
        storage = absolute(args.storage)
        token = token_from(absolute(args.token_file))
        private_dir(storage)
        with locked(storage):
            serve(storage, args.port, token)
        return 0
    if command in {"demo-result", "put", "get"}:
        path = "results/hello-from-host.md" if command == "demo-result" else args.path
        validate_path(path)
        remote = Remote(args.url, token_from(absolute(args.token_file)))
        item = remote.inventory().get(path)
        if command == "get":
            if item is None:
                raise Problem(f"{path}: not on host")
            sys.stdout.buffer.write(remote.download(path, item))
        else:
            data = (f"# Hello from Coven Host\n\nCastle: `{CASTLE}`\n\n"
                    "Demo message: this artifact came from the host, not a live agent.\n").encode() \
                if command == "demo-result" else read_external(absolute(args.source))
            if item is not None and remote.download(path, item) == data:
                print(f"{path}: already current (revision {item[0]})")
            else:
                revision, sha = remote.upload(path, data, item[0] if item else 0)
                print(f"{path}: revision {revision}, SHA-256 {sha}")
        return 0

    state = absolute(args.state)
    client = Client(state)
    if command == "init":
        root = absolute(args.root)
        separate(root, state)
        private_dir(state)
        with locked(state):
            client.initialize(root)
        print(f"Initialized {root}")
        return 0
    if not client.path.is_file():
        raise Problem("client is not initialized; run init with this --state")
    private_dir(state)
    with client.db() as db:
        root = Path(client.setting(db, "root"))
    separate(root, state)
    folder = Folder(root, state)
    if command == "status":
        with locked(state, required=False) as acquired:
            client.status(folder, not acquired)
        return 0
    if command == "watch":
        delay = 2
        previous = None
        while True:
            with locked(state):
                result = client.sync(folder)
            if result != previous:
                print(": ".join(filter(None, result)), flush=True)
                previous = result
            time.sleep(delay if result[0] == "offline" else 2)
            delay = min(delay * 2, 30) if result[0] == "offline" else 2
    with locked(state):
        if command == "connect":
            path = absolute(args.token_file)
            if path.resolve().is_relative_to(root.resolve()):
                raise Problem("keep the token file outside the Coven folder")
            client.config(args.url, path)
            print("Connection configured; run sync to authenticate and reconcile")
        elif command == "sync":
            phase, error = client.sync(folder)
            print(": ".join(filter(None, (phase, error))))
            return 0 if phase == "synced" else 1
        elif command == "resolve":
            choice = "local" if args.local else "remote" if args.remote else "merged"
            merged = read_external(absolute(args.merged)) if args.merged else None
            client.resolve(folder, args.path, choice, merged)
            print("Resolution saved; run sync to confirm convergence. Recovery copies retained.")
    return 0


def main():
    os.umask(0o077)
    try:
        return execute(parser().parse_args())
    except KeyboardInterrupt:
        return 130
    except (OSError, Problem, sqlite3.Error) as exc:
        print(f"coven-host: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
