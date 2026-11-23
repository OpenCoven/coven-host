# Native Host MVP delivery plan

The MVP connects one ordinary folder and one authenticated peer. Its acceptance condition is an installed Coven CLI and production daemon completing offline sync and recovery on macOS/APFS, Linux/ext4 and Windows/NTFS, followed by a successful SSH disconnect, peer restart and reconciliation. Coven Host owns the Rust client library and peer. Coven owns the Rust CLI and daemon. The historical Python revision supplies compatibility fixtures only.

## Offline deletion and rename release

Version 0.1.2 ships offline deletion and rename in the Rust CLI and production daemon. Both endpoints preserve the surviving edit when a deletion races with an edit. Users can inspect the saved bytes and choose the result. [Downloads](https://github.com/OpenCoven/coven-host/releases/tag/v0.1.2) · [Recovery instructions](operations.md#recover-a-deletion-or-rename)

The Rust deletion changes merged to Host main at `16eab38dab601df6a66cbbe36304483725db8834`. Coven `9e14f365e07e57892920d9fed31bbbf9b257b66c` pins Host `c39e4474be545ae64fdb6bc88bcfa7e3443286be` in the CLI manifest and lockfile. The latter Host revision and the reviewed merge contain identical product code. The final Host test reads Windows file permissions from the open handle. [Host merge](https://github.com/OpenCoven/coven-host/pull/18) · [Consumer pin review](https://github.com/OpenCoven/coven/pull/1815#pullrequestreview-6011878642)

The installed review build passed offline renames in both directions, both delete-versus-edit choices and recovery of the exact document bytes. It upgraded v0.1.1 on physical Apple silicon/APFS and native Windows x64/NTFS, recovered a held-open Windows file, and ran on Linux x64 and ARM64/ext4 at the glibc 2.34 minimum. SSH qualification connected the Mac to an ARM64 Linux guest on the same physical machine. After the tunnel dropped and the peer and daemon restarted, the folders converged with the original peer identity. Installed fault tests covered lost acknowledgements, peer rollback, an exhausted deletion archive and a restoration at the folder quota. [Windows GUI and elevated-session diagnostic](https://github.com/OpenCoven/coven-host/actions/runs/45013342553) · [Linux](https://github.com/OpenCoven/coven-host/actions/runs/45013342577) · [Mac](https://github.com/OpenCoven/coven-host/actions/runs/45013342574) · [NTFS permissions](https://github.com/OpenCoven/coven-host/actions/runs/45010990009)

The installer source is `8d1a2437f8035c0eaf712c53248502c4c35ad669`. The packages use the reviewed Coven commit above, which landed on Coven main at `8773e883bebf68bd5cad9209a9f96477458e920b`. The dependency audit passed with `url` 2.5.9 and `form_urlencoded` 1.2.3. [Package hashes and source revisions](https://github.com/OpenCoven/coven-host/releases/download/v0.1.2/installer-receipt.json) · [Release coordination](https://github.com/OpenCoven/coven-host/issues/20)

## Milestones

| Deliverable | Owner | State on 2026-11-23 | Completion evidence or next gate |
| --- | --- | --- | --- |
| Rust peer and client | Coven Host | Implemented and qualified | Python-compatible protocol and SQLite state, identity pinning, conditional writes and durable retry receipts. [Compatibility results](acceptance.md) |
| Safe native replacement | Coven Host | Implemented and qualified | Linux exchange, macOS swap and journaled Windows replacement preserve recoverable bytes across editor races and interruption. [Platform results](acceptance.md) |
| Native CLI, daemon and package integration | Coven | Packaged for the Host release | Daemon owns the root, token reference and polling worker. Same-user IPC advertises `hostSync` and exposes typed host operations. [Integration issue](https://github.com/OpenCoven/coven/issues/1805) |
| Installed CLI and SSH qualification | Host and Coven contributors | Installed runs and same-Mac guest SSH passed. Two physical machines remain open in [#12](https://github.com/OpenCoven/coven-host/issues/12) | See [tested revisions, environments and limits](acceptance.md). |
| Library review and merge | Coven Host maintainer | Merged into main | [Host PR #1](https://github.com/OpenCoven/coven-host/pull/1) records the review history and merge commit on `main`. |
| Desktop installation | Coven Host | [v0.1.2 published](https://github.com/OpenCoven/coven-host/releases/tag/v0.1.2) | First launch, repeat setup, login synchronization, copyable diagnostics and offline installation passed. [Desktop qualification](acceptance.md#desktop-installers) |
| Consumer review and distribution | Coven maintainer | Host installers available, original upstream integration merged | See the [Host release installation](setup.md). The [upstream integration PR](https://github.com/OpenCoven/coven/pull/1809) records the original merge. The deletion release uses the later consumer pin above. |
| Contributor checks | Coven Host | Local runner and pull request CI | Run `scripts/check.sh --fast` from a full checkout. Linux contributors can run `scripts/check.sh --full` for interoperability and upgrade coverage. [Commands and prerequisites](acceptance.md#run-the-checks) |

## Next handoff

The downloaded v0.1.2 release passed offline rename and delete-versus-edit recovery on Apple silicon/APFS, Linux x64 and ARM64/ext4, and x64 Windows/NTFS. The Windows installer runs in a per-user session. [Public installer checks](https://github.com/OpenCoven/coven-host/actions/runs/45050106799) · [Mac and SSH results](https://github.com/OpenCoven/coven-host/issues/5#issuecomment-7082956073)

BunsDev owns [multi-client coordination #6](https://github.com/OpenCoven/coven-host/issues/6). Start with a two-client regression against one peer. Take both clients offline, edit on one and delete on the other, reconnect in each order, and verify revision convergence and recoverable bytes for both resolution choices. Coordinate with BunsDev before editing the shared peer or schema. [Platform and signing handoffs](acceptance.md#remaining-operator-qualification) list the equipment and evidence needed for #9–#12. The Project issuer decision D01 remains with Val and the design owners before [draft PR #16](https://github.com/OpenCoven/coven-host/pull/16) can proceed.

## Scope for follow-up work

Contributors looking for unassigned engineering work can claim [additional Castles #7](https://github.com/OpenCoven/coven-host/issues/7). Begin with a path-policy test for two distinct Castle IDs and their separate quotas. Coordinate peer and schema changes with BunsDev on #6 and keep optional Project metadata separate from Castle identity. [Uploaded document execution #8](https://github.com/OpenCoven/coven-host/issues/8) starts with a reviewed security design.

To verify #5 from a full checkout, run `./tests/compat-linux.sh --deletions` with Git, a Rust toolchain, Python 3.11+ and a filesystem supporting native replacement. It starts disposable Rust endpoints and checks both rename directions, conflict choices, acknowledgement loss and rollback. Run the script without arguments on Linux to include the historical Python pairings and stopped-installation upgrades.
