# Native Host MVP delivery plan

The MVP connects one ordinary folder and one authenticated peer. Its acceptance condition is an installed Coven CLI and production daemon completing offline sync and recovery on macOS/APFS, Linux/ext4 and Windows/NTFS, followed by a successful SSH disconnect, peer restart and reconciliation. Coven Host owns the Rust client library and peer. Coven owns the Rust CLI and daemon. The historical Python revision supplies compatibility fixtures only.

## Milestones

| Deliverable | Owner | State on 2026-11-20 | Completion evidence or next gate |
| --- | --- | --- | --- |
| Rust peer and client | Coven Host | Implemented and qualified | Python-compatible protocol and SQLite state, identity pinning, conditional writes and durable retry receipts. [Compatibility results](acceptance.md) |
| Safe native replacement | Coven Host | Implemented and qualified | Linux exchange, macOS swap and journaled Windows replacement preserve recoverable bytes across editor races and interruption. [Platform results](acceptance.md) |
| Native CLI, daemon and package integration | Coven | Packaged for the Host release | Daemon owns the root, token reference and polling worker. Same-user IPC advertises `hostSync` and exposes typed host operations. [Integration issue](https://github.com/OpenCoven/coven/issues/1805) |
| Installed CLI and two-machine qualification | Host and Coven contributors | Passed for the pinned pair | See [tested revisions, environments and limits](acceptance.md). |
| Library review and merge | Coven Host maintainer | Merged into main | [Host PR #1](https://github.com/OpenCoven/coven-host/pull/1) records the review history and merge commit on `main`. |
| Desktop installation | Coven Host | [v0.1.1 published](https://github.com/OpenCoven/coven-host/releases/tag/v0.1.1) | First launch, repeat setup, login synchronization, copyable diagnostics and offline installation passed. [Desktop qualification](acceptance.md#desktop-installers) |
| Consumer review and distribution | Coven maintainer | Host installers available, upstream integration merged | See the [Host release installation](install.md). The [upstream integration PR](https://github.com/OpenCoven/coven/pull/1809) records the candidate and its qualification results. Coven merged the integration at `6855b5a4`. Distribution of a deletion-capable package remains open. |

## Next handoff

Host PR #1 is merged. The [v0.1.1 installer guide](setup.md) is the entry point for desktop users. The [v0.1.0 release](install.md) supplies the underlying native packages, wrapper and integration source. [Coven PR #1809](https://github.com/OpenCoven/coven/pull/1809) landed with Host pinned at `65927dd5`. See the [landing receipt](https://github.com/OpenCoven/coven/issues/1805) and [acceptance report](acceptance.md) for the scope of the installed evidence. If either implementation changes, identify which compatibility, native package or SSH scenarios the change affects and rerun them. Record the release version when a distributable package is published.

Host #14 landed at `254ef926`. [Host PR #22](https://github.com/OpenCoven/coven-host/pull/22) integrated ownership checks into [PR #18](https://github.com/OpenCoven/coven-host/pull/18). [PR #24](https://github.com/OpenCoven/coven-host/pull/24) also clears a stale conflict when both sides delete the same file. Its Host revision `22f802e76e0c944707bb0dd3b14716916c091f95` was qualified with Coven tree `d30e4742a9e3fbca2c03f96a4697dfe895687e60` on APFS, Linux ext4 and native x64 NTFS, including recovery across SSH to a Linux VM. Run `./tests/compat-linux.sh --deletions` from the Host candidate for the focused regression. [Installed evidence and package digests](https://github.com/OpenCoven/coven-host/issues/20) · [Installer candidate](https://github.com/OpenCoven/coven-host/pull/26)

Nova owns the [consumer repin](https://github.com/OpenCoven/coven/pull/1815#issuecomment-7076463477) after the Host correction is integrated. Verify the resulting manifest, lockfile and package identity against the qualified tree before distribution.

[Agent readiness #4](https://github.com/OpenCoven/coven-host/issues/4) tracks the shared contributor checks and backlog. Issues #9–#12 cover physical Intel, native x64 Windows, installer signing and physical-machine SSH qualification. The Project issuer decision D01 remains with Val and the design owners before [draft PR #16](https://github.com/OpenCoven/coven-host/pull/16) can proceed.

## Scope for follow-up work

The published v0.1.1 installer covers creation, updates and explicit conflict resolution in the demo Castle. The v0.1.2 candidate adds recoverable deletion and rename. Multi-client coordination, additional Castles and execution of uploaded documents remain separately scoped.
