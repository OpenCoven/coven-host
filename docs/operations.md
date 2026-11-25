# Operate a native Coven Host

Uploaded documents synchronize as bytes. Host runs no processor when a file arrives or a peer restarts. An explicit `coven host demo-result` writes fixed Markdown. See [what happens to an uploaded document](uploaded-documents.md) for the offline walkthrough and owner controls.

Use a Coven CLI and daemon built with the `hostSync` capability. The [acceptance report](acceptance.md) identifies the tested build. The following commands use a Unix client and a Linux peer. On Windows, use the same CLI commands with local absolute paths and an owner-only ACL on the token. Keep private client state and the synchronized folder in separate directories on the same volume.

## Connect two clients

Two clients belonging to the same owner can synchronize independent local copies of the demo Castle with one authenticated peer. Each client needs its own folder, daemon profile and private SQLite state. On each computer, initialize the local folder and connect it to the same peer using its own protected token file containing the shared owner credential. Follow the [SSH setup](#connect-through-ssh) for a remote peer. Each client pins the peer identity independently.

If one client edits a file offline and the other deletes it, the first change to reach the peer advances its revision. On the client that reports the conflict, inspect the saved edit with `coven host status`. Use `--local` to keep that client’s outcome or `--remote` to keep the committed peer outcome. Run sync on both clients to converge. The saved edit stays available even when you choose deletion. Both clients have the same authority over the Castle. The shared credential does not provide per-device revocation.

## Add a named Castle

The named-Castles candidate lets one owner use the same Coven root and peer for several Castles. The public v0.1.2 installer supports the demo commands. To check your build, look for `--castle` in `coven host --help`. Run the candidate on each client and on the peer.

For example, keep your existing demo and add `research` to the foreground peer from the SSH walkthrough below. Keep the demo token and peer storage. Create a distinct credential outside the synchronized folder on your client, then copy it through the verified SSH connection. Set `REMOTE` as in the walkthrough. Run this provisioning step once and keep the protected token files for subsequent starts.

```sh
STATE="$HOME/.local/state/coven-host"
(umask 077; set -C; openssl rand -hex 32 > "$STATE/access/research-token")
ssh -o StrictHostKeyChecking=yes "$REMOTE" \
  'umask 077; set -C; cat > "$HOME/.local/state/coven-host/access/research-token"' \
  < "$STATE/access/research-token"
```

On the peer, stop the foreground service from the walkthrough and restart it with both credentials. The same listener and SSH forward now serve both Castles. Keep the list of registrations when restarting the service. This example uses a manually managed SSH peer. The desktop installer's automatic login service starts the demo configuration.

```sh
STATE="$HOME/.local/state/coven-host"
coven host serve --port 8765 --storage "$STATE/server" \
  --token-file "$STATE/access/token" --castle-token "research=$STATE/access/research-token"
```

On the client, use the existing Coven root. If it predates daemon profile ownership, first run `coven host init --root "$HOME/Coven"` from its original configured profile. The selector applies to one command. Commands without a selector continue to use demo.

```sh
STATE="$HOME/.local/state/coven-host"
coven host --castle research init --root "$HOME/Coven"
coven host --castle research connect http://127.0.0.1:18765 --token-file "$STATE/access/research-token"
coven host --castle research sync
coven host --castle research status
```

Save a document in `~/Coven/Castles/research/files/note.md`, sync research again, and read its bytes with the following command. A file with the same name in demo has its own bytes and revision. For continuous updates, run `coven host --castle research watch` in a client terminal. To add a second client, give it a protected copy of the research token and repeat the client steps in its own profile and root.

```sh
coven host --castle research get files/note.md --url http://127.0.0.1:18765 --token-file "$STATE/access/research-token"
```

To check which Castle needs attention after a restart, run both status commands in the profile that owns the folder. Read the `castle` field and the `path` in its conflict entry before opening a recovery copy. Two entries named `files/note.md` can belong to different Castles.

```sh
coven host status
coven host --castle research status
```

If one client deletes the research note while another edits it offline, sync both and inspect `coven host --castle research status` on the client reporting the conflict. Confirm that `castle` is `research` and the entry in `conflicts` names `files/note.md`. Open the path in `local` or `remote` for the surviving edit and copy it to a private location if needed. If the local copy is the edit you want, run `coven host --castle research resolve files/note.md --local` and sync both research clients. For a remote rename, inspect the old file's entry in the selected Castle's `previousVersions`.

Castle IDs have 1–63 lowercase ASCII letters, digits or hyphens, start with a letter, and end with a letter or digit. Windows reserved device names are excluded on every platform. The local manifest's `id` must match its folder name. Compatible optional fields survive reinitialization. A Project reference describes an association and grants no access. Keep the distinct bearer credentials in protected files.

Back up the peer with the service stopped. Demo keeps its original `host.sqlite` directly in the peer storage directory. Research uses `castles/research/host.sqlite` beneath it. Keep the Castle ID with each backup. Restoring a database under a different ID stops the candidate with a Castle identity diagnostic. A historical database without an ID belongs in the original demo location.

## Recover a deletion or rename

The v0.1.1 desktop release preserves a missing synchronized file on the peer for operator attention. The v0.1.2 desktop release negotiates deletion support between the native client and peer. When an editor renames a file offline, the next sync can publish its new path and retire the old path in separate operations. After an interrupted connection, reconnect and run `coven host sync` to finish. [Release status](plan.md#offline-deletion-and-rename-release)

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

Edit `Castles/demo/files/` or `Castles/demo/results/` with any editor. Run `coven host sync` to wait for reconciliation, or `coven host watch` to follow the daemon’s polling loop until interrupted. Use `coven host status` offline to see pending paths, conflict copies and recovery locations. With deletion-capable endpoints, deleting a synchronized file while offline removes its peer copy on the next sync. Renaming it synchronizes the new path and deletes the old one. The two paths reconcile separately, so an interrupted rename can require another sync.

If both sides edited the same file, inspect both paths reported by `status`. Choose `coven host resolve files/note.md --local`, `--remote` or `--merged /path/to/merged.md`, then sync again. A raced or interrupted replacement can also leave private staging files or displaced bytes. Inspect the paths and, on Windows, the recorded destination, staging and backup observations before moving retained copies to a private backup and retrying. The status fields are `retained` and `replacementStatus`.

The native client keeps the displaced file itself after an update. An editor that still has that file open can finish saving to it. Offline status lists these files under `previousVersions`, with the Castle path in each location. Each private archive reserves up to 100 MiB or 1000 previous versions. If it fills, finish saving and close editors, then move older versions you want to keep to a private backup before syncing again.

## Verify recovery across an SSH disconnect

Use a Mac client and an authorized Linux peer on a different physical computer. Verify both desktop v0.1.2 packages against the [release checksums](https://github.com/OpenCoven/coven-host/releases/download/v0.1.2/SHA256SUMS). The exercise uses the installed binaries, private temporary directories and two unused loopback ports. Leave your normal desktop services running. Verify the SSH host key through a trusted channel before starting.

1. **Prepare the client.** In one Mac shell, substitute the verified SSH destination. Keep this shell open. The last command prints the temporary Linux directory for step 2. The SSH connection copies the disposable token without printing it.

```sh
REMOTE=your-user@your-verified-host
CLI="$HOME/Library/Application Support/Coven Host/releases/v0.1.2/bin/coven"
RUN=$(mktemp -d /tmp/coven-host-12.XXXXXX)
export COVEN_HOME="$RUN/profile" COVEN_HOST_STATE="$RUN/client"
TOKEN="$RUN/token"
NOTE="$RUN/root/Castles/demo/files/note.md"
URL=http://127.0.0.1:28765
(umask 077; openssl rand -hex 32 > "$TOKEN")
ssh -o StrictHostKeyChecking=yes "$REMOTE" \
  'umask 077; dir=$(mktemp -d /tmp/coven-host-12.XXXXXX) && cat > "$dir/token" && echo "$dir"' < "$TOKEN"
```

2. **Start the peer.** In a Linux shell set `RUN` to the directory printed above. For the portable package, use the absolute path to its `coven` executable in place of the Debian path. Define these variables and start the peer in that shell. Keep the shell for the restart.

```sh
RUN=/tmp/coven-host-12.REPLACE
CLI=/usr/lib/coven-host-desktop/coven
URL=http://127.0.0.1:28764
TOKEN="$RUN/token"
"$CLI" host serve --storage "$RUN/server" --port 28764 --token-file "$TOKEN"
```

3. **Establish the baseline.** In the original Mac shell open a dedicated SSH forward. Initialize the disposable folder, publish one note and record the peer identity, revision 1 and file digest using the inventory command below. If a chosen port is occupied, pick a free port and update the matching URL and forward.

```sh
forward() {
  ssh -fNT -M -S "$RUN/ssh" -o StrictHostKeyChecking=yes -o ExitOnForwardFailure=yes \
    -L 127.0.0.1:28765:127.0.0.1:28764 "$REMOTE"
}
forward
"$CLI" host init --root "$RUN/root"
"$CLI" host connect "$URL" --token-file "$TOKEN"
printf 'baseline\n' > "$NOTE"
"$CLI" host sync
shasum -a 256 "$NOTE"
```

Run this inventory command in the shell on either endpoint with its own `URL` and `TOKEN`. It requires curl and jq.

```sh
awk '{print "Authorization: Bearer " $0}' "$TOKEN" |
  curl --silent --show-error --fail --header @- "$URL/v1/castles/demo/files" |
  jq -e 'select(.host and (.files | any(.path == "files/note.md"))) | {host, note: [.files[] | select(.path == "files/note.md") | {revision, sha256}]}'
```

4. **Disconnect and edit offline.** In the Mac shell run the following. On Linux interrupt only the foreground peer with Ctrl-C, then repeat its original `host serve` command. In a second Linux shell set the same `RUN`, `CLI`, `URL` and `TOKEN` as in step 2. Run inventory there. The restarted peer must still have its original identity, baseline digest and revision 1.

```sh
ssh -S "$RUN/ssh" -O exit "$REMOTE"
"$CLI" daemon stop
printf 'offline client edit\n' > "$NOTE"
shasum -a 256 "$NOTE"
```

5. **Create the competing edit on Linux.** Run these commands in the second Linux shell while the forward is down. Use inventory again and record the peer edit at revision 2.

```sh
printf 'peer edit\n' > "$RUN/input"
"$CLI" host put files/note.md "$RUN/input" --url "$URL" --token-file "$TOKEN"
sha256sum "$RUN/input"
```

6. **Recover both edits.** In the Mac shell run `forward`, then `"$CLI" host sync`. Expect a conflict. Run `"$CLI" host status` and hash the files at the `local` and `remote` paths for `files/note.md` with `shasum -a 256`. Match them to the client and peer edit hashes respectively before resolving. Run the commands below and repeat inventory. The folder and fetched peer bytes must match the offline edit, the conflict must clear, and a second sync must preserve the identity and revision 3.

```sh
"$CLI" host resolve files/note.md --local
"$CLI" host sync
"$CLI" host get files/note.md --url "$URL" --token-file "$TOKEN" > "$RUN/fetched"
shasum -a 256 "$NOTE" "$RUN/fetched"
"$CLI" host sync
"$CLI" host status
```

Record the platforms and filesystems, general network path, package digests, peer identity, revisions, hashes of both recovery copies and final bytes. Keep tokens and private paths out of the report. In the original Mac shell stop the disposable daemon with `"$CLI" daemon stop` and close the forward with `ssh -S "$RUN/ssh" -O exit "$REMOTE"`. Stop the foreground Linux peer. After saving the evidence, remove the two temporary directories created for this run. [Observed results and platform limits](acceptance.md#ssh-walkthrough)

## Upgrade an existing Linux installation

Stop the Python client and peer before switching an existing installation. Use the existing host storage for `serve`. The default Linux daemon discovers the Python client database at `~/.local/state/coven-host/client`. For a custom Python state directory, set `COVEN_HOST_STATE` to its absolute path in the environment that starts the daemon. The existing root, identity pin, outbox and conflict copies are read in place. `put`, `get` and `demo-result` accept `--url` and `--token-file` and communicate directly with the peer.

## Sync boundaries

- Within each configured Castle, `Castles/<id>/files/` and `Castles/<id>/results/` synchronize in both directions. The versioned `Castles/<id>/castle.json` and top-level `README.md` are local. The manifest describes the Castle.
- Names within those two directories must start with a lowercase ASCII letter, digit, `_` or `-`, then use only those characters plus `.`. A name cannot end with a dot or use the reserved stems `con`, `prn`, `aux`, `nul`, `com1`–`com9`, `lpt1`–`lpt9`. The limits are 120 characters per component, 512 per path and 16 slash-separated components including `files`/`results`. With the exclusions below, unsupported names and nonregular entries (including symbolic and hard links) are reported rather than followed.
- Exclusions apply case-insensitively to every component. Excluded prefixes are `.` (including `.git`, `.env*`, `.ssh`, `.coven` and other dotfiles), `~`, `credentials`, `secrets`, `token`, `id_`. Excluded names are `node_modules`, `__pycache__`, `vendor`, `target`, `cache`, `caches`. Excluded suffixes are `.key`, `.pem`, `.p12`, `.pfx`, `.ppk`, `.kdbx`, `.tmp`, `.temp`, `.swp`, `.part`, `.bak`, `.sqlite`, `.db`, `~`. The filter checks names. Keep credentials outside the synchronized directories.
- Each Castle on both sides allows at most 10 MiB per file, 100 MiB of current document bytes and 1,000 files. The client also caps nonexcluded file and directory entries at 16,000. Recovery has its own 100 MiB / 2,000-copy budget. Allow extra disk space for staging, SQLite and journals. The limits do not constrain an editor's local filesystem writes.
- Deletion requires a previously synchronized path and a matching peer revision. A client accepts a peer deletion only when the authenticated inventory carries a newer deletion revision. An unavailable Castle or unexplained loss of history stops reconciliation. Restore the original peer database after a rollback.
- A local deletion that races with a peer edit, or a local edit that races with a peer deletion, appears under conflicts. Inspect the surviving copy, choose the side to keep with `resolve --local` or `--remote` for that path, then sync. Choosing the deleted side removes the document. The edited bytes stay in recovery. A removed local file also remains under `previousVersions`.
- With an older peer or client, a local rename can upload the new path while leaving the old path unresolved. Upgrade both endpoints before relying on deletion propagation. A stale creation request at revision zero cannot overwrite a deleted path on the new peer.

Each peer token authorizes its holder to access its registered Castle. The client pins the peer identity on its first authenticated inventory. Back up state with the corresponding process stopped. Use the peer API for remote writes so bytes, revisions and retry receipts stay consistent.

The scoped API is rooted at `/v1/castles/<id>/files`. The original demo route is `/v1/castles/demo/files`. An authenticated `GET` lists revisioned items and the host ID. Append `/files/...` or `/results/...` to read an item. A `PUT` to that item requires the host ID, quoted expected revision (`"0"` for creation), content length, SHA-256 and bearer authorization. The shipped `get`, `put` and `demo-result` commands use this interface.


An authenticated inventory request with `X-Coven-Deletions: 1` returns a `deleted` map of absent paths and their revisions on supporting peers. To remove a file on the peer, send `DELETE` to its item URL with bearer authentication, `X-Host-ID`, the quoted current revision in `If-Match`, and a unique 32-character lowercase hexadecimal `X-Sync-Operation`. Retry an uncertain deletion with the same operation ID. Recreating that path requires its deletion revision. The peer archives removed bytes in the `deleted_versions` table of its private database. Archive limits are 2,000 versions and 100 MiB. A full archive rejects the deletion and leaves the file available. Back up the database with the peer stopped.
