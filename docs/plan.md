# Native Host MVP delivery plan

The MVP connects one ordinary folder and one authenticated peer. Its acceptance condition is an installed Coven CLI and production daemon completing offline sync and recovery on macOS/APFS, Linux/ext4 and Windows/NTFS, followed by a successful SSH disconnect, peer restart and reconciliation. Coven Host owns the Rust client library and peer. Coven owns the Rust CLI and daemon. The historical Python revision supplies compatibility fixtures only.

## Offline deletion and rename release

Version 0.1.1 is the distributed desktop installer. The 0.1.2 candidate adds recoverable deletion and rename to the installed Castle folder. The Host correction, consumer pin and installer updates have separate review heads. [Host #24](https://github.com/OpenCoven/coven-host/pull/24) · [Coven #1815](https://github.com/OpenCoven/coven/pull/1815) · [Installer #26](https://github.com/OpenCoven/coven-host/pull/26)

Coven `8fb558c60b006b337bf7850e6e43e0793b788097` pins reviewed Host `247904def96591b7f473a6e9a98ec8363b925da0` in both the CLI manifest and the lockfile. The corrected client keeps unrelated paths moving when the peer deletion archive fills and checks folder limits before restoring an edited file. Both fixes passed in the installed Mac CLI. The same consumer tree passed a v0.1.1 upgrade on physical Apple silicon/APFS, reconciliation after interrupting SSH to an ARM64 Linux guest, and native Linux x64 and ARM64 installer checks at the glibc 2.34 minimum. [Host review](https://github.com/OpenCoven/coven-host/pull/24#pullrequestreview-6011543135) · [Consumer review](https://github.com/OpenCoven/coven/pull/1815#pullrequestreview-6011608575) · [Linux qualification](https://github.com/OpenCoven/coven-host/actions/runs/45008750967)

Two installed Castle folders also exchanged an offline rename in both directions. When one folder deleted a file and the other edited it, choosing the edit restored identical bytes in both folders at peer revision 3. The receiving client retained the old path under `previousVersions`. The downloaded Mac candidate also retried a deletion after a lost acknowledgement and refused to resurrect a file after a peer rollback. [Offline kit hashes](https://github.com/OpenCoven/coven-host/issues/5#issuecomment-7076674853) · [Installed two-folder checks on Linux and Windows](https://github.com/OpenCoven/coven-host/actions/runs/45004210827)

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

For the 0.1.2 release, use issue #20 to coordinate the stacked Host changes, the pinned consumer and the installer. The release owner records native package hashes and fresh-install and upgrade results there, merges the reviewed revisions, and repeats both recovery journeys from the downloaded release. The Windows installer runs in a per-user session. On a group-owned Windows token, setup reports CH-W12 before writing the installation. [Release coordination](https://github.com/OpenCoven/coven-host/issues/20) · [Canonical Windows build](https://github.com/OpenCoven/coven-host/actions/runs/45010508872)

[Agent readiness #4](https://github.com/OpenCoven/coven-host/issues/4) tracks the shared contributor checks and backlog. Issues #9–#12 cover physical Intel, native x64 Windows, installer signing and physical-machine SSH qualification. The Project issuer decision D01 remains with Val and the design owners before [draft PR #16](https://github.com/OpenCoven/coven-host/pull/16) can proceed.

## Scope for follow-up work

This iteration covers creation, updates and explicit conflict resolution in the demo Castle. Missing files remain visible as unresolved work. Multi-client coordination, propagated deletion, additional Castles and execution of uploaded documents would change the operating contract and belong in separately scoped issues.
