# Native Host MVP delivery plan

The MVP connects one ordinary folder and one authenticated peer. Its acceptance condition is an installed Coven CLI and production daemon completing offline sync and recovery on macOS/APFS, Linux/ext4 and Windows/NTFS, followed by a successful SSH disconnect, peer restart and reconciliation. Coven Host owns the Rust client library and peer. Coven owns the Rust CLI and daemon. The historical Python revision supplies compatibility fixtures only.

## Milestones

| Deliverable | Owner | State on 2026-11-18 | Completion evidence or next gate |
| --- | --- | --- | --- |
| Rust peer and client | Coven Host | Implemented and qualified | Python-compatible protocol and SQLite state, identity pinning, conditional writes and durable retry receipts. [Compatibility results](acceptance.md) |
| Safe native replacement | Coven Host | Implemented and qualified | Linux exchange, macOS swap and journaled Windows replacement preserve recoverable bytes across editor races and interruption. [Platform results](acceptance.md) |
| Native CLI, daemon and package integration | Coven | Packaged for the private Host release | Daemon owns the root, token reference and polling worker. Same-user IPC advertises `hostSync` and exposes typed host operations. [Integration issue](https://github.com/OpenCoven/coven/issues/1805) |
| Installed CLI and two-machine qualification | Host and Coven contributors | Passed for the pinned pair | See [tested revisions, environments and limits](acceptance.md). |
| Library review and merge | Coven Host maintainer | Tracked in PR #1 | [Host PR #1](https://github.com/OpenCoven/coven-host/pull/1) records the review history and merge commit on `main`. |
| Desktop installation | Coven Host | v0.1.1 desktop installers and offline kit | Qualify first launch, repeat setup, login synchronization and copyable diagnostics on each platform. [Desktop installation](setup.md) |
| Consumer review and distribution | Coven maintainer | Private Host installation available, upstream Coven review pending | See the [Host release installation](install.md). Submit the integration for [Coven #1805](https://github.com/OpenCoven/coven/issues/1805) when authorized, verify the dependency resolves from a clean checkout, then follow Coven’s review and release process. |

## Next handoff

Host PR #1 is merged. The v0.1.0 release distributes the three native packages, wrapper and integration source in this repository. Upstream review belongs to the Coven maintainer on [Coven #1805](https://github.com/OpenCoven/coven/issues/1805). Carry the tested library pin and integration commit from the [acceptance report](acceptance.md) into Coven review. If either implementation changes, identify which compatibility, native package or SSH scenarios the change affects and rerun them. Record the release version when a distributable package is published.

## Scope for follow-up work

This iteration covers creation, updates and explicit conflict resolution in the demo Castle. Missing files remain visible as unresolved work. Multi-client coordination, propagated deletion, additional Castles and execution of uploaded documents would change the operating contract and belong in separately scoped issues.
