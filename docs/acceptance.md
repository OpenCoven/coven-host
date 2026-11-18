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

The annotated release tag signs the SHA-256 digest of the release asset named `SHA256SUMS`. Use the verified tag in the private repository as the reference. Compare the digest printed for the manifest with the value in the signed tag message, then compare the selected installer with its entry in that manifest. The checksum file inside the offline ZIP covers the files in the extracted kit.

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
