# Native MVP acceptance

On 2026-11-18, the native CLI and production daemon passed packaged acceptance on three filesystems. The packages were installed from locally built npm tarballs. The [v0.1.0 private release](install.md) repackaged those same executables and repeated installed acceptance on all three platforms, including the SSH walkthrough. The [plan](plan.md) tracks upstream Coven distribution.

| Component | Tested revision |
| --- | --- |
| Rust library consumed by Coven | [`732d8c3a06fb51fc32ade8eba46ab384c3924968`](https://github.com/OpenCoven/coven-host/commit/732d8c3a06fb51fc32ade8eba46ab384c3924968) |
| Windows test cleanup after qualification | [`113544b68fe74e068e87ad6b460d619ef15506c6`](https://github.com/OpenCoven/coven-host/commit/113544b68fe74e068e87ad6b460d619ef15506c6) changed only `tests/folder.rs`. |
| Coven CLI, daemon and packaging | Local commit `40a5a18ed7abdaff6c6072695ecc0c186f4c0cd1` on `feat/1805-native-host`. [Coven #1805](https://github.com/OpenCoven/coven/issues/1805) |

## Installed packages

| Client and volume | Observed result | Installed executable SHA-256 |
| --- | --- | --- |
| Apple Silicon macOS, APFS | 16 scenarios passed, including older-daemon negotiation, concurrent sync, case aliases, writes through an open editor handle and interrupted replacement. | `e34b85823a362e6fef541adb70d6e6787f5eb9515dbc16ae859fb24d19806b18` |
| x86-64 Linux guest, ext4 | 17 scenarios passed, including rollback beneath an open conflict, restart, an edit racing exchange and offline discovery of displaced bytes. | `04acbac673092f29dbfce0a66a08e3b185c9246ceb4ae05381ffb1a18b03f934` |
| Windows 11 guest, NTFS | Eight scenarios passed, including six concurrent callers, junctions, case aliases, a real sharing violation, late editor saves and daemon termination during an update. | `1e7a6e20030d01492e60607ec7b9916a26b4bdbc4ccc2c1594877fd50c5786cf` |

Windows library tests also interrupted the process immediately after replacement and after backup promotion, then inspected the destination, staging file and backup. The locked Coven workspace suite recorded 5,908 passed, zero failed and 34 ignored.

## SSH walkthrough

The macOS client opened a strict host-key-checked SSH forward to a Linux guest. With the tunnel down, a local edit survived a daemon restart. The peer restarted on its original database and retained its identity and revision 1. After reconnect, the offline edit became revision 2. Conflicting edits produced two recoverable versions. Explicit local resolution left both endpoints at revision 4 with SHA-256 `6089e19ae7647e2b425a5648590d62b37dc92d8b8cb957bb0e68dde52e552903`.

The walkthrough passed against x86-64 and ARM Linux guests. The client and guests shared one physical Mac. To repeat it, follow the [SSH setup](operations.md#connect-through-ssh), record the peer identity and file hash before disconnection, then compare the identity, revision and file bytes on reconnect and after resolution.

## Linux compatibility

The Python client passed all ten reference scenarios against the Rust peer. The Rust client passed nine reference scenarios against each peer implementation. Native filesystem race tests exercise the exchange case that the reference suite injects into Python. Rust also opened stopped Python state with a pending upload acknowledgement and an open conflict, and detected both an older peer snapshot and a changed digest at the same conflict revision.

## Run the checks

From the repository root, run the Rust checks on the target OS. The example binary adapts the production library to the Python test harness.

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --locked --example compat_client
```

On Linux with Python 3.11+ and a filesystem supporting atomic exchange, run the three interoperability combinations and the in-place upgrade. The script extracts the historical reference from Git into a temporary directory and removes it on exit. A shallow checkout must fetch the fixture commit named by the script first. Python is needed only for this test suite.

```sh
./tests/compat-linux.sh
```

When reporting a failed case, include the first error, peer revision and which copies survived.
