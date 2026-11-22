# Native Host MVP delivery plan

The MVP connects one ordinary folder and one authenticated peer. Its acceptance condition is an installed Coven CLI and production daemon completing offline sync and recovery on macOS/APFS, Linux/ext4 and Windows/NTFS, followed by a successful SSH disconnect, peer restart and reconciliation. Coven Host owns the Rust client library and peer. Coven owns the Rust CLI and daemon. The historical Python revision supplies compatibility fixtures only.

## Offline deletion and rename release

Version 0.1.1 is the distributed desktop installer. The 0.1.2 candidate adds recoverable deletion and rename to the installed Castle folder. The Host correction, consumer pin and installer updates have separate review heads. [Host #24](https://github.com/OpenCoven/coven-host/pull/24) · [Coven #1815](https://github.com/OpenCoven/coven/pull/1815) · [Installer #26](https://github.com/OpenCoven/coven-host/pull/26)

The Rust deletion changes merged to Host main at `16eab38dab601df6a66cbbe36304483725db8834`. Coven `29bb4f964f257d3f1cf8a3e2e32ab5657f65dd3b` pins Host `c39e4474be545ae64fdb6bc88bcfa7e3443286be` in the CLI manifest and lockfile. The latter Host revision and the reviewed merge contain identical product code. The final Host test reads Windows file permissions from the open handle. [Host merge](https://github.com/OpenCoven/coven-host/pull/18) · [Consumer pin review](https://github.com/OpenCoven/coven/pull/1815#pullrequestreview-6011722102)

The installed review build passed offline renames in both directions, both delete-versus-edit choices and recovery of the exact document bytes. It upgraded v0.1.1 on physical Apple silicon/APFS and native Windows x64/NTFS, recovered a held-open Windows file, and ran on Linux x64 and ARM64/ext4 at the glibc 2.34 minimum. SSH qualification connected the Mac to an ARM64 Linux guest on the same physical machine. After the tunnel dropped and the peer and daemon restarted, the folders converged with the original peer identity. Installed fault tests covered lost acknowledgements, peer rollback, an exhausted deletion archive and a restoration at the folder quota. [Windows GUI and elevated-session diagnostic](https://github.com/OpenCoven/coven-host/actions/runs/45011686994) · [Linux](https://github.com/OpenCoven/coven-host/actions/runs/45011072027) · [Mac](https://github.com/OpenCoven/coven-host/actions/runs/45011072026) · [NTFS permissions](https://github.com/OpenCoven/coven-host/actions/runs/45010990009)

The 0.1.2 release also requires the `url` 2.5.9 security update in the production CLI’s lockfile. The corrected dependency tree passes the advisory, license, ban and source checks. The release owner records the final consumer SHA, installer hashes and post-publication download results on #20. [Dependency audit and correction](https://github.com/OpenCoven/coven/pull/1815#pullrequestreview-6011742530) · [Release coordination](https://github.com/OpenCoven/coven-host/issues/20)

## Milestones

| Deliverable | Owner | State on 2026-11-20 | Completion evidence or next gate |
| --- | --- | --- | --- |
| Rust peer and client | Coven Host | Implemented and qualified | Python-compatible protocol and SQLite state, identity pinning, conditional writes and durable retry receipts. [Compatibility results](acceptance.md) |
| Safe native replacement | Coven Host | Implemented and qualified | Linux exchange, macOS swap and journaled Windows replacement preserve recoverable bytes across editor races and interruption. [Platform results](acceptance.md) |
| Native CLI, daemon and package integration | Coven | Packaged for the Host release | Daemon owns the root, token reference and polling worker. Same-user IPC advertises `hostSync` and exposes typed host operations. [Integration issue](https://github.com/OpenCoven/coven/issues/1805) |
| Installed CLI and two-machine qualification | Host and Coven contributors | Passed for the pinned pair | See [tested revisions, environments and limits](acceptance.md). |
| Library review and merge | Coven Host maintainer | Merged into main | [Host PR #1](https://github.com/OpenCoven/coven-host/pull/1) records the review history and merge commit on `main`. |
| Desktop installation | Coven Host | [v0.1.1 published](https://github.com/OpenCoven/coven-host/releases/tag/v0.1.1) | First launch, repeat setup, login synchronization, copyable diagnostics and offline installation passed. [Desktop qualification](acceptance.md#desktop-installers) |
| Consumer review and distribution | Coven maintainer | Host installers available, original upstream integration merged | See the [Host release installation](install.md). The [upstream integration PR](https://github.com/OpenCoven/coven/pull/1809) records the original merge. The deletion release uses the later consumer pin above. |

## Next handoff

The v0.1.1 installer and the original upstream CLI integration are available. Host PR #14 merged the profile ownership guard and Coven PR #1809 merged the upstream integration. [Desktop setup](setup.md) · [Profile ownership](https://github.com/OpenCoven/coven-host/pull/14) · [Upstream integration](https://github.com/OpenCoven/coven/pull/1809)

For the 0.1.2 release, use issue #20 to coordinate the merged Host changes, the pinned consumer and the installer. The release owner records native package hashes and fresh-install and upgrade results there, merges the reviewed revisions, and repeats both recovery journeys from the downloaded release. The Windows installer runs in a per-user session. On a group-owned Windows token, setup reports CH-W12 before writing the installation. [Release coordination](https://github.com/OpenCoven/coven-host/issues/20) · [Patched Windows qualification](https://github.com/OpenCoven/coven-host/actions/runs/45013342553)

[Agent readiness #4](https://github.com/OpenCoven/coven-host/issues/4) tracks the shared contributor checks and backlog. Issues #9–#12 cover physical Intel, native x64 Windows, installer signing and physical-machine SSH qualification. The Project issuer decision D01 remains with Val and the design owners before [draft PR #16](https://github.com/OpenCoven/coven-host/pull/16) can proceed.

## Scope for follow-up work

[Host #5](https://github.com/OpenCoven/coven-host/issues/5) adds deletion and rename reconciliation to the demo Castle. The v0.1.1 installers use the earlier behavior. The reviewed 0.1.2 consumer pin appears above. Multi-client coordination, additional Castles and execution of uploaded documents have separate tracking issues.

To verify #5 from a full checkout, run `./tests/compat-linux.sh --deletions` with Git, a Rust toolchain, Python 3.11+ and a filesystem supporting native replacement. It starts disposable Rust endpoints and checks both rename directions, conflict choices, acknowledgement loss and rollback. Run the script without arguments on Linux to include the historical Python pairings and stopped-installation upgrades.
