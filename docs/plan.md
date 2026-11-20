# Native Host MVP delivery plan

The MVP connects one ordinary folder and one authenticated peer. Its acceptance condition is an installed Coven CLI and production daemon completing offline sync and recovery on macOS/APFS, Linux/ext4 and Windows/NTFS, followed by a successful SSH disconnect, peer restart and reconciliation. Coven Host owns the Rust client library and peer. Coven owns the Rust CLI and daemon. The historical Python revision supplies compatibility fixtures only.

## Milestones

| Deliverable | Owner | State on 2026-11-20 | Completion evidence or next gate |
| --- | --- | --- | --- |
| Rust peer and client | Coven Host | Implemented and qualified | Python-compatible protocol and SQLite state, identity pinning, conditional writes and durable retry receipts. [Compatibility results](acceptance.md) |
| Safe native replacement | Coven Host | Implemented and qualified | Linux exchange, macOS swap and journaled Windows replacement preserve recoverable bytes across editor races and interruption. [Platform results](acceptance.md) |
| Native CLI, daemon and package integration | Coven | Packaged for the Host release | Daemon owns the root, token reference and polling worker. Same-user IPC advertises `hostSync` and exposes typed host operations. [Integration issue](https://github.com/OpenCoven/coven/issues/1805) |
| Installed CLI and two-machine qualification | Host and Coven contributors | Passed for the pinned pair | See [tested revisions, environments and limits](acceptance.md). |
| Agent readiness and CI | Coven Host | In review on 2026-11-19 | `scripts/check.sh` is the single check entrypoint, and CI runs it on Linux, macOS and Windows with a Linux compatibility job and a secret scan. Follow-up work is filed as claimable issues #5–#12. [Tracking issue #4](https://github.com/OpenCoven/coven-host/issues/4) |
| Library review and merge | Coven Host maintainer | Merged into main | [Host PR #1](https://github.com/OpenCoven/coven-host/pull/1) records the review history and merge commit on `main`. |
| Desktop installation | Coven Host | [v0.1.1 published](https://github.com/OpenCoven/coven-host/releases/tag/v0.1.1) | First launch, repeat setup, login synchronization, copyable diagnostics and offline installation passed. [Desktop qualification](acceptance.md#desktop-installers) |
| Consumer review and distribution | Coven maintainer | Host installers available, upstream integration in review | See the [Host release installation](install.md). The [upstream integration PR](https://github.com/OpenCoven/coven/pull/1809) records the candidate and its qualification results. Maintainer review and Coven’s release process remain. |

## Next handoff

Host PR #1 is merged. The [v0.1.1 installer guide](setup.md) is the entry point for desktop users. The [v0.1.0 release](install.md) supplies the underlying native packages, wrapper and integration source. The Coven maintainer owns review of [Coven PR #1809](https://github.com/OpenCoven/coven/pull/1809) for [Coven #1805](https://github.com/OpenCoven/coven/issues/1805). Use the PR for its current library pin and checks, and the [acceptance report](acceptance.md) for the shipped Host packages. If either implementation changes, identify which compatibility, native package or SSH scenarios the change affects and rerun them. Record the release version when a distributable package is published.

## Scope for follow-up work

This iteration covers creation, updates and explicit conflict resolution in the demo Castle. Missing files remain visible as unresolved work. Multi-client coordination, propagated deletion, additional Castles and execution of uploaded documents would change the operating contract and belong in separately scoped issues. Each of these items, and each untested platform in the acceptance report, has a claimable issue listed on [#4](https://github.com/OpenCoven/coven-host/issues/4).
