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
| Consumer review and distribution | Coven maintainer | Host installers available, upstream integration in review | See the [Host release installation](install.md). The [upstream integration PR](https://github.com/OpenCoven/coven/pull/1809) records the candidate and its qualification results. Maintainer review and Coven’s release process remain. |

## Next handoff

Host PR #1 is merged. The [v0.1.1 installer guide](setup.md) is the entry point for desktop users. The [v0.1.0 release](install.md) supplies the underlying native packages, wrapper and integration source. The Coven maintainer owns review of [Coven PR #1809](https://github.com/OpenCoven/coven/pull/1809) for [Coven #1805](https://github.com/OpenCoven/coven/issues/1805). Use the PR for its current library pin and checks, and the [acceptance report](acceptance.md) for the shipped Host packages. If either implementation changes, identify which compatibility, native package or SSH scenarios the change affects and rerun them. Record the release version when a distributable package is published.

The maintainer’s next decision is whether to accept the profile ownership change in [Host PR #14](https://github.com/OpenCoven/coven-host/pull/14) and the integration in [Coven PR #1809](https://github.com/OpenCoven/coven/pull/1809). Coven pins Host `65927dd5`. The installed candidate was `350f186f` and the consumer review head `6caf1095` adds the dependency audit configuration. The candidate passed installed APFS, ext4, NTFS and SSH qualification. [Qualification handoff](https://github.com/OpenCoven/coven/issues/1805#issuecomment-7006154995)

[Agent readiness #4](https://github.com/OpenCoven/coven-host/issues/4) tracks the shared contributor checks and backlog. Issues #9–#12 cover physical Intel, native x64 Windows, installer signing and physical-machine SSH qualification. The Project issuer decision D01 remains with Val and the design owners before [draft PR #16](https://github.com/OpenCoven/coven-host/pull/16) can proceed.

## Scope for follow-up work

[Host #5](https://github.com/OpenCoven/coven-host/issues/5) adds deletion and rename reconciliation to the demo Castle. The Coven consumer pin and distributed packages still use the earlier behavior. Multi-client coordination, additional Castles and execution of uploaded documents have separate tracking issues.

To verify #5 from a full checkout, run `./tests/compat-linux.sh --deletions` with Git, a Rust toolchain, Python 3.11+ and a filesystem supporting native replacement. It starts disposable Rust endpoints and checks both rename directions, conflict choices, acknowledgement loss and rollback. Run the script without arguments on Linux to include the historical Python pairings and stopped-installation upgrades.


## Installed deletion qualification handoff, November 22

Host deletion and recovery landed at `16eab38d`. The signed [Coven consumer PR](https://github.com/OpenCoven/coven/pull/1815) at `29bb4f96` pins Host `c39e4474`. Its tree `5844d2c3a22e2622aa59ec05ca011aad938e6306` is the one exercised by the staged installers. The [qualification report](acceptance.md#installed-offline-deletion-candidate) gives the package hashes, operator scenarios and reproduction commands. BunsDev owns consumer review and distribution. Track the desktop release separately in [installer PR #26](https://github.com/OpenCoven/coven-host/pull/26) and [coordination issue #20](https://github.com/OpenCoven/coven-host/issues/20).
