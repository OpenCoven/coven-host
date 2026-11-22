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

## Desktop installers

The v0.1.1 installers configure the native CLI, a private peer and a folder watcher. Every supported installer completed a fresh setup, an automatic upload and download, and a repeat setup with the original credentials. The [installation guide](setup.md) lists the download for each system.

| System exercised | Observed installation behavior |
| --- | --- |
| Apple silicon Mac, macOS 27.2.1 | Setup installed a durable app under the current account. After the original download was removed, reopening that app preserved the login services and credentials. The earlier v0.1.0 profile retained its peer data. A pre-existing folder and an unrelated app with the same name survived the diagnostic path. The Intel executable and installer logic ran under Rosetta. Physical Intel hardware was outside this run. |
| Windows 11 ARM64 build 26300, NTFS, x64 emulation | The installer opened the folder in Explorer and restricted token access to the owner. After a reboot and sign-in, the watcher resumed and uploaded the exact contents of an offline edit. Copy and Save produced the same diagnostic report. The release EXE is unsigned. |
| Ubuntu 24.04, x86-64 and ARM64 | Debian packages and portable archives transferred bytes in both directions. The x86-64 desktop run exercised graphical package installation, folder opening, clipboard diagnostics and recovery from conflicting edits under Xvfb. The x86-64 fallback run exercised operation without a systemd user manager. |

The platform chooser passed browser checks for supported and unsupported systems, clipboard denial, report saving and a narrow viewport. With networking disabled, the extracted offline kit resolved every installer and its checksum file with JavaScript enabled and disabled. A missing-file report directed the user to re-extract the kit.

Apple accepted the signed macOS app. After stapling the ticket, the release maintainer extracted the final archive and verified its code signature for both architectures. Gatekeeper accepted it as Notarized Developer ID from Soul Protocol LLC. The shipping app uses the signed-in account. The private test profile is confined to the separate verifier. [Apple submission run](https://github.com/OpenCoven/chat/actions/runs/44323270011)

### Verify a release download

The annotated release tag signs the SHA-256 digest of the release asset named `SHA256SUMS`. Use the verified release tag as the reference. Compare the digest printed for the manifest with the value in the signed tag message, then compare the selected installer with its entry in that manifest. The checksum file inside the offline ZIP covers the files in the extracted kit.

With the Windows installer and the release manifest in Downloads, open PowerShell in that directory and run the following commands. The final two lines should show the same installer digest, ignoring letter case.

```powershell
Get-FileHash .\SHA256SUMS -Algorithm SHA256
Get-FileHash .\Coven-Host-0.1.1-Windows-x64.exe -Algorithm SHA256
Select-String -Path .\SHA256SUMS -Pattern '  Coven-Host-0\.1\.1-Windows-x64\.exe$'
```

On macOS, run `shasum -a 256 SHA256SUMS` to check the manifest itself. After extracting the offline kit, run `shasum -a 256 -c SHA256SUMS` from the extracted folder. On Linux, use `sha256sum` in place of `shasum -a 256`.

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

On Linux with Python 3.11+ and a filesystem supporting atomic exchange, run the three interoperability combinations and the in-place upgrade. The script extracts the historical reference from Git into a temporary directory and removes it on exit. A shallow checkout must fetch the fixture commit named by the script first. The sync engine and peer run natively in Rust. The Linux desktop setup launcher also uses Python.

```sh
./tests/compat-linux.sh
```

When reporting a failed case, include the first error, peer revision and which copies survived.


## Installed offline deletion candidate

Host [#18](https://github.com/OpenCoven/coven-host/pull/18) merged as `16eab38dab601df6a66cbbe36304483725db8834`. The consumer [#1815](https://github.com/OpenCoven/coven/pull/1815) at `29bb4f964f257d3f1cf8a3e2e32ab5657f65dd3b` pins Host `c39e4474be545ae64fdb6bc88bcfa7e3443286be` in both the CLI manifest and lockfile. The installer workflows asserted the resulting Git tree `5844d2c3a22e2622aa59ec05ca011aad938e6306` before building. Host review head `3b86f79d` additionally changes a Windows regression under the test configuration. These are staged 0.1.2 review packages.

| Platform and installer | Package SHA-256 | Installed verification |
| --- | --- | --- |
| macOS 27.2.1 arm64, APFS, universal application ZIP | `2d65a770edefdf7f81a2b1164d81d40b32ff8e06ebae9e3161769812a9fa893a` | Verified the downloaded app signature and ran its bundled arm64 CLI in a private prefix. Executable SHA-256 `bdd0e699ec25219faf19dd45eafb70180d7773320c354d282f577c7cbd5df0bc`. [Build and qualification](https://github.com/OpenCoven/coven-host/actions/runs/45011072026) |
| Linux arm64, ext4, Debian package | `aedc79e23a04eee4f47593fb143c6c4cb4efcf2ae7219e07f232a33f1467e593` | Installed with dpkg and ran the production CLI from its installed path. Executable SHA-256 `ff2736e9c17e411c220c177c39134842246cefff8f9fd7e29c26ffbe984b3fbc`. [Build and qualification](https://github.com/OpenCoven/coven-host/actions/runs/45011072027) |
| Linux x64, ext4, Debian package | `81293842379e88d52f879ea195bb370e18e75727c09dad53a0f7e869cbcdd4d6` | Hosted native x64 installed CLI, daemon and desktop scenarios passed. [Linux matrix](https://github.com/OpenCoven/coven-host/actions/runs/45011072027) |
| Windows Server native x64, NTFS, setup EXE | `87bb1f5dba9ef8ec91e964d9211182e5c5da5c8223004c7fc5241c47f3b360bd` | Standard-user install and upgrade passed. The elevated-account diagnostic timed out waiting for a watcher record. The downloaded EXE subsequently passed the elevated control and both standard-user runs. [Initial run](https://github.com/OpenCoven/coven-host/actions/runs/45011072040) · [Downloaded EXE verification](https://github.com/OpenCoven/coven-host/actions/runs/45011686994) |

### Reproduce the operator scenarios

From a full Host checkout, pass the absolute installed CLI path to the qualification script below. It creates temporary profiles, protected tokens, a local authenticated peer and a production daemon. Put `TMPDIR` on the target volume. The script starts and stops only the processes for its disposable profiles. Peer-side removals use the revision-conditional HTTP API with a unique operation ID.

```sh
python3 tests/installed_profile_recovery.py "$INSTALLED_COVEN"
```

The APFS and ARM64 ext4 runs exercised a competing profile before migration, then the owning profile reopening, migrating and syncing. A rename and a deletion from each endpoint survived disconnect and restart. For local-delete versus remote-edit and local-edit versus remote-delete, both choices restored the expected live state, preserved the exact edited bytes at the reported recovery path and converged on repeat sync. The test asserts revisions 2 or 3 for those choices and compares content hashes. For ordinary deletion it also checks the peer archive and recovers displaced local files from previous versions.

The candidate packages in the table can be reproduced from the pinned build workflows and retrieved from their Actions artifacts while retained. For example, the following downloads and verifies the full Linux arm64 artifact before extracting the Debian package into an empty qualification directory.

```sh
gh api repos/OpenCoven/coven-host/actions/artifacts/14750275286/zip > linux-arm64.zip
echo '7858539c7cdcb86ed369e835723da75c8e1a2eabe181acfc462a019c8392f102  linux-arm64.zip' | sha256sum -c -
unzip linux-arm64.zip
sha256sum Coven-Host-0.1.2-linux-arm64.deb
```

For the integrated Host revision run `cargo test --locked --test client --test peer`, `./tests/compat-linux.sh --deletions` and `./tests/compat-linux.sh` on Linux. Both source platforms passed seven client tests, six peer tests and nine focused deletion cases. The Linux suite discovered 25 tests in each of its three pairings, with 11, 12 and 1 expected skips, including the stopped Python upgrade on the native pairing. The local full run used `CARGO_PROFILE_DEV_OPT_LEVEL=1` after the earlier default debug run timed out on a 10 MiB quota PUT. The hosted Linux compatibility job passed the default suite for the consumer pin. [Hosted compatibility](https://github.com/OpenCoven/coven-host/actions/runs/45011072027)

The previous npm candidate at consumer `89f05444` passed installed APFS scenarios and timed out during daemon startup in a local emulated x86-64 guest. Its evidence and digests belong to the earlier Host pin. BunsDev owns Windows follow-up on [#10](https://github.com/OpenCoven/coven-host/issues/10), the consumer merge decision and release acceptance on [#20](https://github.com/OpenCoven/coven-host/issues/20).
