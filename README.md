# Coven Host

Coven Host synchronizes an ordinary folder with one authenticated peer. The Rust library implements the peer protocol and client reconciliation used by the native `coven host` commands. The Python implementation below remains the Linux compatibility reference.

## Sync a folder with the native CLI

Use a Coven build with the `hostSync` capability. Start the peer on the Linux host with a protected token file and a private storage directory. The service binds to IPv4 loopback and runs independently of a desktop or daemon.

```sh
coven host serve --port 8765 --storage "$HOME/.local/state/coven-host/server" --token-file "$HOME/.local/state/coven-host/access/token"
```

On the client, open the verified SSH tunnel described below. The client and server token files must contain the same URL-safe token of 43–128 characters. Keep the client token outside the folder. On Unix, the token must be owned by the current user with mode 600 or stricter. On Windows, protect it with an owner-only ACL.

```sh
coven host init --root "$HOME/Coven"
coven host connect http://127.0.0.1:18765 --token-file "$HOME/.local/state/coven-host/access/token"
coven host sync
coven host status
```

Edit `Castles/demo/files/` or `Castles/demo/results/` with any editor. Run `coven host sync` to wait for reconciliation, or `coven host watch` to follow the daemon’s polling loop until interrupted. Use `coven host status` offline to see pending paths, conflict copies and recovery locations. Deleting or renaming a synchronized file leaves the old remote copy available for resolution.

If both sides edited the same file, inspect both paths reported by `status`. Choose `coven host resolve files/note.md --local`, `--remote` or `--merged /path/to/merged.md`, then sync again. A raced or interrupted replacement can also leave private staging files or displaced bytes. Inspect the paths and, on Windows, the recorded destination, staging and backup observations before moving retained copies to a private backup and retrying.

Stop the Python client and peer before switching an existing installation. Use the existing host storage for `serve`. The default Linux daemon discovers the Python client database at `~/.local/state/coven-host/client`. For a custom Python state directory, set `COVEN_HOST_STATE` to its absolute path in the environment that starts the daemon. The existing root, identity pin, outbox and conflict copies are read in place. `put`, `get` and `demo-result` accept `--url` and `--token-file` and communicate directly with the peer.

## Python reference walkthrough

Use Linux with Python 3.11+ and a filesystem supporting atomic
`renameat2(RENAME_EXCHANGE)` (tested on Ubuntu Linux with Python 3.12.3).
Client state and the Coven folder must be on the same filesystem in separate
directories. Other operating systems and filesystems are unverified.
There are no packages to install. From the repository checkout:

```sh
python3 -m compileall -q coven_host
python3 -m coven_host --help
python3 -m unittest discover -s tests -v
```

The following disposable walkthrough uses Bash, port 8765, and separate client
and server processes. It does not touch `~/Coven`. The token is generated into
a private file, never printed or passed as an argument.

```bash
DEMO=$(mktemp -d)
(umask 077; python3 -c 'import secrets; print(secrets.token_urlsafe(32))' > "$DEMO/access")
host() { python3 -m coven_host --state "$DEMO/client" "$@"; }
start_host() {
  python3 -m coven_host serve --storage "$DEMO/server" --token-file "$DEMO/access" &
  SERVER_PID=$!
}
start_host                       # Wait for "Serving Castle demo ...".
host init --root "$DEMO/Coven"    # Safe to rerun; never overwrites existing files.
host connect http://127.0.0.1:8765 --token-file "$DEMO/access"
CASTLE="$DEMO/Coven/Castles/demo"

# Open $CASTLE/files in your file browser, or edit from the shell.
printf 'notes from this computer\n' > "$CASTLE/files/notes.md"
host sync
host get --token-file "$DEMO/access" files/notes.md | sha256sum
sha256sum "$CASTLE/files/notes.md"  # Same hash.
host demo-result --token-file "$DEMO/access"
host sync
cat "$CASTLE/results/hello-from-host.md"

# Work offline, then restart the host using the same storage.
kill "$SERVER_PID"; wait "$SERVER_PID" || :
printf 'notes edited offline\n' > "$CASTLE/files/notes.md"
host sync                         # Expected: offline, exit 1.
host status
start_host                        # Wait for the serving message again.
host sync                         # Expected: synced.

# Make divergent edits before the next reconciliation.
printf 'local edit\n' > "$CASTLE/files/notes.md"
printf 'host edit\n' > "$DEMO/host-note.md"
host put --token-file "$DEMO/access" files/notes.md "$DEMO/host-note.md"
host sync                         # Expected: unresolved conflict, exit 1.
host status                       # Prints paths to both retained versions.
host resolve files/notes.md --local
host sync
host get --token-file "$DEMO/access" files/notes.md
host status

# Optional continuous sync: host watch (stop with Ctrl-C).
# Stop the host separately:
kill "$SERVER_PID"; wait "$SERVER_PID" || :
```

To keep the host version choose `--remote`; to supply bytes from a merged file,
choose `--merged /absolute/path/to/merged.md`. Stop `watch` before manually
resolving. After resolution, run `sync` to confirm convergence. Recovery copies
remain in the displayed recovery directory. Before removing resolved copies
from recovery, move them to a private backup; do not remove copies referenced
by an open conflict.

## Deploy on a Linux host over SSH

Use an authorized remote machine and an unprivileged user. Put the reviewed
Coven Host checkout at `~/coven-host` on each machine. Use your normal SSH
authentication, and verify the host key against a trusted fingerprint before
adding it to `known_hosts`; the commands below require an already verified key.
On the **client**, from the checkout:

```sh
REMOTE=your-user@your-verified-host
STATE="$HOME/.local/state/coven-host"
install -d -m 700 "$STATE/access"
(umask 077; set -C; python3 -c 'import secrets; print(secrets.token_urlsafe(32))' > "$STATE/access/token")
ssh -o StrictHostKeyChecking=yes "$REMOTE" \
  'umask 077; install -d -m 700 "$HOME/.local/state/coven-host/access"; set -C; cat > "$HOME/.local/state/coven-host/access/token"' \
  < "$STATE/access/token"
```

These token-creation commands refuse to overwrite existing files. Reuse an
existing matching pair on subsequent runs. Do not copy tokens into either
checkout or into the Coven tree. On the **remote**, start the service in its
own terminal:

```sh
cd "$HOME/coven-host"
python3 -m coven_host serve --port 8765 \
  --storage "$HOME/.local/state/coven-host/server" \
  --token-file "$HOME/.local/state/coven-host/access/token"
```

Leave storage in place across restarts. Stop the foreground process with
Ctrl-C. The service binds only to `127.0.0.1`; do not open port 8765 publicly.
In another **client** terminal, start the verified tunnel, and keep it open:

```sh
ssh -NT -o StrictHostKeyChecking=yes -o ExitOnForwardFailure=yes \
  -L 127.0.0.1:18765:127.0.0.1:8765 your-user@your-verified-host
```

Then in the **client checkout**:

```sh
python3 -m coven_host init
python3 -m coven_host connect http://127.0.0.1:18765 \
  --token-file "$HOME/.local/state/coven-host/access/token"
python3 -m coven_host sync
python3 -m coven_host watch       # Ctrl-C stops the client; it does not stop SSH or the host.
```

Use `init --root /absolute/path` with a separate `--state /absolute/path`
(global option, before the command) to use another local folder. On the
**remote**, run `python3 -m coven_host demo-result --token-file
"$HOME/.local/state/coven-host/access/token"` from the checkout to publish the
demo artifact. The client also accepts a certificate-verified `https://`
origin if you already operate a TLS reverse proxy to the loopback service.
Never forward loopback HTTP without an authenticated tunnel or expose the
Python service directly.

## Scope, limits and recovery

- Only `Castles/demo/files/` and `Castles/demo/results/` synchronize; both are
  bidirectional. The versioned `Castles/demo/castle.json` and top-level
  `README.md` are local. The manifest is data, not an authority grant.
- Names within those two directories must start with a lowercase ASCII
  letter, digit, `_` or `-`, then use only those characters plus `.`. A name
  cannot end with a dot or use the reserved stems `con`, `prn`, `aux`, `nul`,
  `com1`–`com9`, `lpt1`–`lpt9`. The limits are 120 characters per component,
  512 per path and 16 slash-separated components including `files`/`results`.
  With the exclusions below, unsupported names and nonregular entries (including
  symbolic and hard links) are reported rather than followed.
- Exclusions apply case-insensitively to every component. Prefixes: `.`
  (including `.git`, `.env*`, `.ssh`, `.coven` and other dotfiles), `~`,
  `credentials`, `secrets`, `token`, `id_`. Exact names: `node_modules`,
  `__pycache__`, `vendor`, `target`, `cache`, `caches`. Suffixes: `.key`,
  `.pem`, `.p12`, `.pfx`, `.ppk`, `.kdbx`, `.tmp`, `.temp`, `.swp`, `.part`,
  `.bak`, `.sqlite`, `.db`, `~`. This is a name filter, **not** a secret scanner:
  never put credentials under a syncable name.
- Both sides allow at most 10 MiB per file, 100 MiB of current document bytes
  and 1,000 files. The client also caps nonexcluded file and directory
  entries at 16,000. Recovery has its own 100 MiB / 2,000-copy budget;
  provisioning needs extra disk space for staging, SQLite and journals.
  The limits do not constrain an editor's local filesystem writes.
- Create and update only. A rename is a new upload and an unresolved
  deletion. A missing previously synced file is not sent as a delete or
  automatically restored. To restore it deliberately, use `get` and save
  the bytes back to the original path, or restore your local backup; then
  sync again. An unavailable tree stops reconciliation. If the host's
  identity or revision history changed, recover the original host database
  instead of pointing existing state at an empty replacement.
- `status` shows the last observed outcome, current local work, last good host
  inventory time and recovery paths. It does not contact the host, so `synced`
  is not a live connectivity probe. `sync` exits 0 only after successful
  reconciliation, otherwise 1. `watch` normally polls every two seconds and
  backs off on connection failures up to 30 seconds. Requests use a
  five-second socket timeout.
- Downloads are verified by revision, size and SHA-256, staged outside the
  tree and installed atomically. If a replacement is interrupted or an edit
  races the final exchange, `status` lists the retained file. Inspect and
  move it outside the recovery directory, restore or merge it if needed,
  then sync again. The local account and its parent directories are trusted;
  pause sync for bulk rearrangements, and do not rely on it to coordinate
  arbitrary writes through an already-open file descriptor.
- The private token represents one owner of this one Castle. The client pins
  the host storage identity after the first authenticated inventory. Client
  state stores the baseline and one durable upload outbox; server SQLite
  commits file bytes, revision and the single client's retry receipt together.
  Do not point two clients at the same host. Back up state when the respective
  processes are stopped. **Direct changes to server storage are unsupported.**

## Coven integration and implementation choices

The original repository contained only a README and license; there was no
local `AGENTS.md` or Host implementation. `OpenCoven/coven` uses same-user
local IPC for agent sessions; `OpenCoven/coven-castles` describes read-only
sandbox preflight, not a document namespace or file authority. The linked
`coven-agents` repository was not accessible from this environment, so no
agent-service contract was assumed. The original prototype uses Python's
standard library for the CLI and loopback HTTP, and SQLite for transactional
storage and restart recovery. The Rust integration above adds the host commands to the main CLI.

The demo is not a live familiar or mission runner. A future authorized
integration can read explicitly selected inputs from `files/` and publish
artifacts under `results/`. The scoped API is rooted at
`/v1/castles/demo/files`: `GET` lists revisioned items and the host ID; `GET`
with `/files/...` or `/results/...` appended reads bytes; `PUT` to that item
requires the host ID, quoted expected revision (`"0"` for creation), content
length, SHA-256 and bearer authorization. The shipped `get`, `put` and
`demo-result` commands use this interface. The API does not authorize or run
missions, execute documents, manage provider sandboxes or schedule work.

## Historical Python acceptance results

Tested implementation: `9bcd2c8d79a165c4a603eeebd2eb8da3b45e6304`.
Environment: Ubuntu 24.04.5, Linux x86_64, Python 3.12.3, SQLite 3.45.1.
The tests launched the client and host as separate processes over IPv4
loopback, with disposable roots and independent persistent state directories.

| Scenario | Observed result |
| --- | --- |
| Local create/update | **Pass.** Binary content, inventory SHA-256, byte length and revisions matched across upload and update. |
| Remote create/update | **Pass.** Nested binary artifact hashes matched both times. The demo result arrived with `Castle: demo` and a labeled demo message. |
| Offline editing and restart | **Pass.** `status` reported offline with one pending path; local bytes remained readable. The host retained the original version after being killed and restarted. A fresh foreground client uploaded the offline edit as revision 2. |
| Concurrent edits | **Pass.** Both versions were recoverable; repeated sync did not duplicate them. Local, remote and merged resolutions each converged, with recovery copies retained. |
| Lost acknowledgement and interrupted download | **Pass.** A loopback proxy dropped a committed PUT response, then truncated a download. The upload receipt survived a host restart and intervening host update; retry neither duplicated the upload nor created a false conflict. The truncated download left the old file untouched. |
| Authentication and root boundaries | **Pass.** Missing and wrong tokens were rejected on listing, read and write. Traversal, path aliases, wrong Castle and symlink targets were rejected. A simulated edit at atomic exchange was retained and reported. |
| Exclusions and limits | **Pass.** Excluded contents stayed local; both sides rejected over-limit files. An 11th 10 MiB file was refused after 100 MiB without being committed. |
| Deletion and unavailable root | **Pass.** Rename created a new remote file and reported the unresolved deletion. Missing root reported unknown pending work without changing remote bytes. A rolled-back host DB did not replace newer local bytes. |

Commands: `python3 -m compileall -q coven_host tests`,
`python3 -m unittest discover -s tests -v` (10 passed), `git diff --check`,
and the disposable Bash walkthrough above. In the walkthrough, local and
host reads both yielded SHA-256
`3609839b83599c2fd78fab31d3f4c1214ce3cbc77aa65d9de69e392db486af78`.
After resolving the deliberate conflict, the host contained `local edit` and
status reported `synced`, zero pending paths and zero conflicts. CodeQL
reported zero Python alerts.

**At the time of that Python run**, no authorized remote machine was configured, so
the SSH deployment on a second machine and the HTTPS reverse-proxy option
are unverified. Run the deployment steps above on an authorized Linux host
and repeat the walkthrough through the verified SSH tunnel, including
stopping and reconnecting the tunnel. Other platforms and graphical file
browsers were not exercised.

## Run the Rust compatibility checks

```sh
cargo test --locked --all-targets
cargo build --locked --example compat_client
NATIVE="$PWD/target/debug/examples/compat_client"
COVEN_TEST_HOST="$NATIVE" python3 -m unittest discover -s tests -v
COVEN_TEST_CLIENT="$NATIVE" python3 -m unittest discover -s tests -v
COVEN_TEST_HOST="$NATIVE" COVEN_TEST_CLIENT="$NATIVE" COVEN_TEST_NATIVE="$NATIVE" python3 -m unittest discover -s tests -v
```

Run the mixed-language matrix on Linux. The upgrade test creates a Python client with a pending upload and an open conflict, stops the Python peer, and opens both databases in Rust. Platform-specific Rust tests exercise replacement and interruption through the native filesystem APIs.
