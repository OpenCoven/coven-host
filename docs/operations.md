# Operate a native Coven Host

Use a Coven CLI and daemon built with the `hostSync` capability. The [acceptance report](acceptance.md) identifies the tested build. The following commands use a Unix client and a Linux peer. On Windows, use the same CLI commands with local absolute paths and an owner-only ACL on the token. Keep private client state and the synchronized folder in separate directories on the same volume.

## Recover a deletion or rename

The v0.1.1 desktop release preserves a missing synchronized file on the peer for operator attention. The 0.1.2 candidate negotiates deletion support between the native client and peer. When an editor renames a file offline, the next sync can publish its new path and retire the old path in separate operations. After an interrupted connection, reconnect and run `coven host sync` to finish. [Release status](plan.md#offline-deletion-and-rename-release)

If one side deletes a file and the other side edits it, run `coven host status`. The conflict entry identifies the deleted side, peer revision and recovery path for the surviving edit. Copy the recovered bytes if needed. Choose the outcome with `coven host resolve files/note.md --local` or `coven host resolve files/note.md --remote`, then run `coven host sync`. Selecting the deleted side removes the file at the synchronized path. If both copies have since been deleted, the next sync clears the conflict and keeps the previously reported recovery file.

If another Castle folder renames a file while this one is offline, reconnect and let the watcher finish. The new path contains the synchronized bytes. For a previously synchronized file removed at its old path, run `coven host status` and inspect the matching entry in `previousVersions` to find the displaced local copy.

## Connect through SSH

Install the host-capable CLI on both systems. Verify the remote SSH host key against a trusted fingerprint and add it to the client’s known hosts. On the client, substitute your SSH destination below. This setup uses OpenSSL to create a matching token pair outside the folder and refuses to overwrite either token. Reuse an existing matching pair on subsequent starts.

```sh
REMOTE=your-user@your-verified-host
STATE="$HOME/.local/state/coven-host"
install -d -m 700 "$STATE/access"
(umask 077; set -C; openssl rand -hex 32 > "$STATE/access/token")
ssh -o StrictHostKeyChecking=yes "$REMOTE" \
  'umask 077; install -d -m 700 "$HOME/.local/state/coven-host/access"; set -C; cat > "$HOME/.local/state/coven-host/access/token"' \
  < "$STATE/access/token"
```

Start the peer in a terminal on the Linux host. Keep its storage across restarts. This foreground service binds to IPv4 loopback and runs independently of the Coven daemon.

```sh
coven host serve --port 8765 --storage "$HOME/.local/state/coven-host/server" --token-file "$HOME/.local/state/coven-host/access/token"
```

In another client terminal, set the same SSH destination and keep the forward open.

```sh
REMOTE=your-user@your-verified-host
ssh -NT -o StrictHostKeyChecking=yes -o ExitOnForwardFailure=yes \
  -L 127.0.0.1:18765:127.0.0.1:8765 "$REMOTE"
```

Tokens must contain 43–128 URL-safe characters. On Unix the current user must own the token file and its mode must be 600 or stricter.

## Sync and recover

Run on the client.

```sh
coven host init --root "$HOME/Coven"
coven host connect http://127.0.0.1:18765 --token-file "$HOME/.local/state/coven-host/access/token"
coven host sync
coven host status
```

Edit `Castles/demo/files/` or `Castles/demo/results/` with any editor. Run `coven host sync` to wait for reconciliation, or `coven host watch` to follow the daemon’s polling loop until interrupted. Use `coven host status` offline to see pending paths, conflict copies and recovery locations. Deleting or renaming a synchronized file leaves the old remote copy available for resolution.

If both sides edited the same file, inspect both paths reported by `status`. Choose `coven host resolve files/note.md --local`, `--remote` or `--merged /path/to/merged.md`, then sync again. A raced or interrupted replacement can also leave private staging files or displaced bytes. Inspect the paths and, on Windows, the recorded destination, staging and backup observations before moving retained copies to a private backup and retrying. The status fields are `retained` and `replacementStatus`.

The native client keeps the displaced file itself after an update. An editor that still has that file open can finish saving to it. Offline status lists these files under `previousVersions`, with the Castle path in each location. Each private archive reserves up to 100 MiB or 1000 previous versions. If it fills, finish saving and close editors, then move older versions you want to keep to a private backup before syncing again.

## Upgrade an existing Linux installation

Stop the Python client and peer before switching an existing installation. Use the existing host storage for `serve`. The default Linux daemon discovers the Python client database at `~/.local/state/coven-host/client`. For a custom Python state directory, set `COVEN_HOST_STATE` to its absolute path in the environment that starts the daemon. The existing root, identity pin, outbox and conflict copies are read in place. `put`, `get` and `demo-result` accept `--url` and `--token-file` and communicate directly with the peer.

## Sync boundaries

- Only `Castles/demo/files/` and `Castles/demo/results/` synchronize in both directions. The versioned `Castles/demo/castle.json` and top-level `README.md` are local. The manifest describes the Castle.
- Names within those two directories must start with a lowercase ASCII letter, digit, `_` or `-`, then use only those characters plus `.`. A name cannot end with a dot or use the reserved stems `con`, `prn`, `aux`, `nul`, `com1`–`com9`, `lpt1`–`lpt9`. The limits are 120 characters per component, 512 per path and 16 slash-separated components including `files`/`results`. With the exclusions below, unsupported names and nonregular entries (including symbolic and hard links) are reported rather than followed.
- Exclusions apply case-insensitively to every component. Excluded prefixes are `.` (including `.git`, `.env*`, `.ssh`, `.coven` and other dotfiles), `~`, `credentials`, `secrets`, `token`, `id_`. Excluded names are `node_modules`, `__pycache__`, `vendor`, `target`, `cache`, `caches`. Excluded suffixes are `.key`, `.pem`, `.p12`, `.pfx`, `.ppk`, `.kdbx`, `.tmp`, `.temp`, `.swp`, `.part`, `.bak`, `.sqlite`, `.db`, `~`. The filter checks names. Keep credentials outside the synchronized directories.
- Both sides allow at most 10 MiB per file, 100 MiB of current document bytes and 1,000 files. The client also caps nonexcluded file and directory entries at 16,000. Recovery has its own 100 MiB / 2,000-copy budget. Allow extra disk space for staging, SQLite and journals. The limits do not constrain an editor's local filesystem writes.
- Create and update only. A rename is a new upload and an unresolved deletion. A missing previously synced file is not sent as a delete or automatically restored. To restore it deliberately, use `get` and save the bytes back to the original path, or restore your local backup, then sync again. An unavailable tree stops reconciliation. If the host's identity or revision history changed, recover the original host database instead of pointing existing state at an empty replacement.

One token authorizes one owner and one client for the demo Castle. The client pins the peer identity on its first authenticated inventory. Back up state with the corresponding process stopped. Use the peer API for remote writes so bytes, revisions and retry receipts stay consistent.

The scoped API is rooted at `/v1/castles/demo/files`. An authenticated `GET` lists revisioned items and the host ID. Append `/files/...` or `/results/...` to read an item. A `PUT` to that item requires the host ID, quoted expected revision (`"0"` for creation), content length, SHA-256 and bearer authorization. The shipped `get`, `put` and `demo-result` commands use this interface.
