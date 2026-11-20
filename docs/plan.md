# Native Host MVP delivery plan

The MVP connects one ordinary folder and one authenticated peer. Its acceptance condition is an installed Coven CLI and production daemon completing offline sync and recovery on macOS/APFS, Linux/ext4 and Windows/NTFS, followed by a successful SSH disconnect, peer restart and reconciliation. Coven Host owns the Rust client library and peer. Coven owns the Rust CLI and daemon. The historical Python revision supplies compatibility fixtures only.

## Milestones

| Deliverable | Owner | State on 2026-11-18 | Completion evidence or next gate |
| --- | --- | --- | --- |
| Rust peer and client | Coven Host | Implemented and qualified | Python-compatible protocol and SQLite state, identity pinning, conditional writes and durable retry receipts. [Compatibility results](acceptance.md) |
| Safe native replacement | Coven Host | Implemented and qualified | Linux exchange, macOS swap and journaled Windows replacement preserve recoverable bytes across editor races and interruption. [Platform results](acceptance.md) |
| Native CLI, daemon and package integration | Coven | Packaged for the private Host release | Daemon owns the root, token reference and polling worker. Same-user IPC advertises `hostSync` and exposes typed host operations. [Integration issue](https://github.com/OpenCoven/coven/issues/1805) |
| Installed CLI and two-machine qualification | Host and Coven contributors | Passed for the pinned pair | See [tested revisions, environments and limits](acceptance.md). |
| Library review and merge | Coven Host maintainer | Merged into main | [Host PR #1](https://github.com/OpenCoven/coven-host/pull/1) records the review history and merge commit on `main`. |
| Desktop installation | Coven Host | [v0.1.1 published](https://github.com/OpenCoven/coven-host/releases/tag/v0.1.1) | First launch, repeat setup, login synchronization, copyable diagnostics and offline installation passed. [Desktop qualification](acceptance.md#desktop-installers) |
| Consumer review and distribution | Coven maintainer | Private Host installation available, upstream Coven review pending | See the [Host release installation](install.md). The integration for [Coven #1805](https://github.com/OpenCoven/coven/issues/1805) is in [Coven PR #1809](https://github.com/OpenCoven/coven/pull/1809). Its pinned Host dependency resolves from a clean checkout. |

## Next handoff

Host PR #1 is merged. The [v0.1.1 installer guide](setup.md) is the entry point for desktop users. The [v0.1.0 release](install.md) supplies the underlying native packages, wrapper and integration source. The maintainer’s next decision is whether to accept the profile ownership change in [Host PR #14](https://github.com/OpenCoven/coven-host/pull/14) and the integration in [Coven PR #1809](https://github.com/OpenCoven/coven/pull/1809). Coven pins Host `65927dd5`. The installed candidate was `350f186f` and the consumer review head `6caf1095` adds the dependency audit configuration. The candidate passed installed APFS, ext4, NTFS and SSH qualification. [Qualification handoff](https://github.com/OpenCoven/coven/issues/1805#issuecomment-7006154995)

[Agent readiness #4](https://github.com/OpenCoven/coven-host/issues/4) tracks the shared contributor checks and backlog. Issues #9–#12 cover physical Intel, native x64 Windows, installer signing and physical-machine SSH qualification. The Project issuer decision D01 remains with Val and the design owners before [draft PR #16](https://github.com/OpenCoven/coven-host/pull/16) can proceed.

## Scope for follow-up work

This iteration covers creation, updates and explicit conflict resolution in the demo Castle. Missing files remain visible as unresolved work. Multi-client coordination, propagated deletion, additional Castles and execution of uploaded documents would change the operating contract and belong in separately scoped issues.
