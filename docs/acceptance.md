# Native MVP acceptance

On 2026-11-18, the native CLI and production daemon passed packaged acceptance on three filesystems. The packages were installed from locally built npm tarballs. The [v0.1.0 private release](install.md) repackaged those same executables and repeated installed acceptance on all three platforms, including the SSH walkthrough. The [plan](plan.md) tracks upstream Coven distribution.

| Component | Tested revision |
| --- | --- |
| Rust library consumed by Coven | [`732d8c3a06fb51fc32ade8eba46ab384c3924968`](https://github.com/OpenCoven/coven-host/commit/732d8c3a06fb51fc32ade8eba46ab384c3924968) |
| Windows test cleanup after qualification | [`113544b68fe74e068e87ad6b460d619ef15506c6`](https://github.com/OpenCoven/coven-host/commit/113544b68fe74e068e87ad6b460d619ef15506c6) changed only `tests/folder.rs`. |
| Coven CLI, daemon and packaging | Local commit `40a5a18ed7abdaff6c6072695ecc0c186f4c0cd1` on `feat/1805-native-host`. [Coven #1805](https://github.com/OpenCoven/coven/issues/1805) |

## Public npm CLI

On 2026-11-25 UTC, a clean npm prefix on Apple silicon installed the public wrapper and native macOS package at 0.5.0 from the npm registry. Both package records identify Coven `d9b9d6a8b9f4775ad7fa0a2d95a9a78004c6d92c`. The installed CLI reported v0.5.0. Its help had no Host subcommand, and the noninteractive `coven host --help` invocation exited 1. The desktop installer has its own release and supplies demo sync. [Registry package](https://registry.npmjs.org/@opencoven%2fcli/0.5.0) · [Desktop installation](setup.md)

| Verified bytes | SHA-256 |
| --- | --- |
| npm wrapper tarball, 0.5.0 | `1264b7150169d4a2c085dacd375b72448dcc59d3be94d4734b9934268b033d28` |
| npm macOS arm64 tarball, 0.5.0 | `b568595e69a907f799b1d0c8bd34bc193ba456db012dfa1c3bf9a3aa9b421204` |
| Installed macOS arm64 executable, 0.5.0 | `d8d0d800602cf708d85074c905a55632703aadf2a1f6edcc9ed8f72b063cf684` |

## Named Castle candidate

The published v0.1.2 macOS installer initialized demo in a disposable profile and rejected `--castle research` with exit code 2. Host `d031b333498c06250fd9410f702401851caa330f` passed the hosted Rust checks on Windows/NTFS, macOS/APFS and Linux, the commit guard and secret scan, and the Linux compatibility matrix. A fresh macOS checkout passed `scripts/check.sh --fast`. The full Linux command passed all four steps on ARM64, including both stopped historical-state upgrade scenarios. [Host checks](https://github.com/OpenCoven/coven-host/actions/runs/45296840728) · [Installed baseline](https://github.com/OpenCoven/coven-host/issues/7#issuecomment-7117761703)

On physical Apple silicon with macOS 27.2.1 and APFS, the desktop installer engine installed the ad hoc signed arm64 candidate in a disposable login profile. The candidate CLI identifies itself as `0.1.3-dev.7`. Coven source `f723ecdf06374c171c7518e1ae8776a5e95ea584` pins the Host revision above in both the CLI manifest and lockfile. The macOS installer archive SHA-256 is `366e5da6c13337db6fd04ee86e63d4743bd1c012730c088dc73abd79ab9ee4af`. The installed executable SHA-256 is `7daf32b8d2e7df7549a66c7a8858a3cf24b418cbbe217d7eef1529d99aefd4f2`. The public installer remains v0.1.2. [Installer receipt and scenarios](https://github.com/OpenCoven/coven-host/issues/7#issuecomment-7118195870)

The installed candidate opened an existing v0.1.2 demo client and peer database in place, synchronized demo, initialized research under the same root, connected it using its own token, and read back the research document. The demo document, manifest including optional Project metadata, profile ownership marker, and demo credential retained their original hashes. This exercised the existing data with the candidate CLI installed in a separate disposable profile. The installer used its existing v0.1.2 package layout.

The installed Mac candidate followed the operator walkthrough through a strict SSH forward to an ARM64 Linux guest on the same physical Mac. Both executables carried the Coven revision above. The Linux peer executable SHA-256 was `6bb559233849d4232b82b48d7b493836664ff8f6f1e048391e8b1a873b989f42`. After loss of the tunnel and a peer and daemon restart, the two original Castle identities returned. Research recovered both copies of an overlapping edit and resolved at revision 3. Demo retained its bytes at revision 1. Two-physical-machine acceptance remains in [#12](https://github.com/OpenCoven/coven-host/issues/12).

The Rust peer tests populated the demo index to its 1,000-file limit, observed HTTP 413 on its next insertion, and read back a new research file on the same listener. Each token was refused access to the other inventory and document. Both Castles replayed the same operation ID independently after restart. In both the Rust adapter and the installed production CLI/daemon journey, one Castle recovered an offline edit at revision 3 after a lost deletion acknowledgement while the other propagated a rename at revision 1. The suite repeated the journey with the roles reversed and checked the exact bytes, both revision 2 deletion archives, and the local recovery copies. The installed CLI also passed all four original demo two-client conflict orderings and decisions. [Reproducible commands](../AGENTS.md#source-and-verification-map)

The local folder check filled research to its 1,000-file limit, added one result, and observed the quota diagnostic while demo still reported its pending file. Removing the result restored the research count. A separate fresh Mac installation passed all three contributor commands, including refusal of a different daemon profile and successful migration by the original owner. [Quota test on three platforms](https://github.com/OpenCoven/coven-host/pull/31) · [Installed CLI and daemon review](https://github.com/OpenCoven/coven/pull/1830#pullrequestreview-6032133866)

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

### Desktop installer v0.1.2

Public downloads passed the signed release manifest checks. The stapled Mac app passed Gatekeeper and upgraded v0.1.1 on Apple silicon/APFS. Two Castle folders converged after disconnected renames in both directions. A delete-versus-edit conflict preserved the edited bytes after explicit resolution. The same release recovered across an SSH disconnect to an ARM64 Linux guest on the Mac. [Release and checksums](https://github.com/OpenCoven/coven-host/releases/tag/v0.1.2) · [Mac recovery evidence](https://github.com/OpenCoven/coven-host/issues/5#issuecomment-7082956073)

The published offline kit installed on native Linux x64 and ARM64 runners using ext4. On an x64 Windows Server runner using NTFS, a standard user upgraded from v0.1.1, recovered a held-open file after releasing its handle and completed both sync journeys. The Windows EXE is unsigned. Physical Intel Mac and separate physical-host SSH qualification remain open. [Public Windows and Linux download runs](https://github.com/OpenCoven/coven-host/actions/runs/45050106799) · [Exact package and build identities](https://github.com/OpenCoven/coven-host/releases/download/v0.1.2/installer-receipt.json)

### Verify a release download

The annotated release tag signs the SHA-256 digest of the release asset named `SHA256SUMS`. Use the verified release tag as the reference. Compare the digest printed for the manifest with the value in the signed tag message, then compare the selected installer with its entry in that manifest. The checksum file inside the offline ZIP covers the files in the extracted kit.

With the Windows installer and the release manifest in Downloads, open PowerShell in that directory and run the following commands. The final two lines should show the same installer digest, ignoring letter case.

```powershell
Get-FileHash .\SHA256SUMS -Algorithm SHA256
Get-FileHash .\Coven-Host-0.1.2-Windows-x64.exe -Algorithm SHA256
Select-String -Path .\SHA256SUMS -Pattern '  Coven-Host-0\.1\.2-Windows-x64\.exe$'
```

On macOS, run `shasum -a 256 SHA256SUMS` to check the manifest itself. After extracting the offline kit, run `shasum -a 256 -c SHA256SUMS` from the extracted folder. On Linux, use `sha256sum` in place of `shasum -a 256`.

## Two-client recovery

The public v0.1.2 installer contains Coven CLI and production daemon build `9e14f365e07e57892920d9fed31bbbf9b257b66c`, using Host library `c39e4474be545ae64fdb6bc88bcfa7e3443286be`. On physical Apple silicon/APFS, the notarized Mac ZIP (SHA-256 `0d94c02d04566ab2184abed21833bc6add81583b492820015fa4e9b8cc407ecf`) installed into a scratch home. Two independent daemon profiles, state databases and Castle roots connected to the same peer. [Release assets and source receipt](https://github.com/OpenCoven/coven-host/releases/tag/v0.1.2)

The downloaded AMD64 and ARM64 Debian packages passed the same four cases as ordinary users on native Ubuntu runners using ext4. Both packages also passed the independent profile-before-migration verifier from PR #27. [Installed Linux checks](https://github.com/OpenCoven/coven-host/actions/runs/45289998169)

Two additional runs of the ARM64 package in a local container encountered a daemon IPC reset during status and a lifecycle-health timeout during initialization. The historical quota test also timed out in that guest. The hosted Linux runs used the original deadlines. [Qualification notes](https://github.com/OpenCoven/coven-host/issues/6#issuecomment-7117211723) · [Full Linux suite](https://github.com/OpenCoven/coven-host/actions/runs/45290371269)

Both clients started at revision 1, disconnected, and made competing changes. The first client's response was dropped after the peer committed revision 2. The peer and clients restarted. The other client observed the conflict before the first retried its operation. The retry used the same operation ID. Each case used a fresh fixture, verified both client databases and the peer database, and repeated sync to check that the revision stayed stable. Every resolution preserved the edited bytes with SHA-256 `9cf697f3bab3fbb3fdcfe9e84b9ba63c40bd13211c11fc8a06b84b9b48c3219c`.

| First change at the peer | Owner keeps | Both folders and peer after resolution |
| --- | --- | --- |
| Delete | Deletion | File absent at revision 2. The peer archive retains the original bytes. |
| Delete | Edit | Edited bytes restored at revision 3. The peer archive retains the original bytes. |
| Edit | Deletion | File absent at revision 3. The peer archive retains the edited bytes. |
| Edit | Edit | Edited bytes present at revision 2. The conflict copy remains recoverable. |

The [contributor guide](../AGENTS.md#source-and-verification-map) gives the installed and source verification commands. The earlier Windows Server and same-Mac SSH results are recorded separately in the [desktop installer results](#desktop-installers).

## SSH walkthrough

The macOS client opened a strict host-key-checked SSH forward to a Linux guest. With the tunnel down, a local edit survived a daemon restart. The peer restarted on its original database and retained its identity and revision 1. After reconnect, the offline edit became revision 2. Conflicting edits produced two recoverable versions. Explicit local resolution left both endpoints at revision 4 with SHA-256 `6089e19ae7647e2b425a5648590d62b37dc92d8b8cb957bb0e68dde52e552903`.

The walkthrough passed against x86-64 and ARM Linux guests. The client and guests shared one physical Mac. To repeat it, follow the [SSH setup](operations.md#connect-through-ssh), record the peer identity and file hash before disconnection, then compare the identity, revision and file bytes on reconnect and after resolution.

## Remaining operator qualification

| Issue | Prerequisite | Evidence to record |
| --- | --- | --- |
| [#9](https://github.com/OpenCoven/coven-host/issues/9) | Physical Intel Mac and universal installer | Record macOS version and executable digest, fresh and repeat setup, both transfer directions, login sync and copyable diagnostics. The Intel run under Rosetta used an Apple silicon host. |
| [#10](https://github.com/OpenCoven/coven-host/issues/10) | Native x64 Windows 11 hardware with NTFS | BunsDev owns the complete installed and desktop scenario set plus folder and library tests. Record Windows build, volume, installer digest, results and inapplicable cases. Hosted x64 Windows Server installation, upgrade and recovery results are recorded separately in the release handoff. |
| [#11](https://github.com/OpenCoven/coven-host/issues/11) | Signing custodian with a protected Authenticode identity | BunsDev coordinates a signed test EXE, valid `Get-AuthenticodeSignature` result, documented verification and passing `scripts/check.sh kit`. |
| [#12](https://github.com/OpenCoven/coven-host/issues/12) | Two separate physical machines, SSH access and disposable profiles | Record both hosts and the network path, then compare peer identity, revisions and hashes after disconnect, peer restart, reconnect and conflict resolution. |

To repeat installed deletion recovery, run `python3 tests/installed_profile_recovery.py "$INSTALLED_COVEN"` with the absolute path to the installed CLI and `TMPDIR` on the volume under test. The harness starts disposable profiles, an authenticated peer and the production daemon. It checks competing-profile refusal before migration, offline rename and deletion in both directions, both conflict choices, exact displaced bytes and repeat-sync convergence. Match your installer digest to the [published release receipt](https://github.com/OpenCoven/coven-host/releases/download/v0.1.2/installer-receipt.json).

## Linux compatibility

The Python client passed all ten reference scenarios against the Rust peer. The Rust client passed nine reference scenarios against each peer implementation. Native filesystem race tests exercise the exchange case that the reference suite injects into Python. Rust also opened stopped Python state with a pending upload acknowledgement and an open conflict, and detected both an older peer snapshot and a changed digest at the same conflict revision.

## Run the checks

From the repository root, run `scripts/check.sh` on the target OS. The pull request workflow calls its named steps across three operating systems.

| Command | Steps | Requirements |
| --- | --- | --- |
| `scripts/check.sh` or `scripts/check.sh --fast` | `rust`, `hook`, `kit` | Rust toolchain, Gitleaks 8.30.1 or a compatible v8 release, OpenSSL, Python 3.11+ |
| `scripts/check.sh --full` | Adds `compat` | Linux with Python 3.11+ and a filesystem supporting atomic exchange, such as ext4 |
| `scripts/check.sh STEP...` | The named steps only | Requirements of those steps |

The steps run these checks:

- `rust` runs `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked --all-targets` and `cargo build --locked --example compat_client`. The example binary adapts the production library to the Python test harness.
- `hook` runs `./tests/pre-commit.sh`, which exercises the commit guard in a disposable repository.
- `kit` runs the offline-kit regression test in `tests/test_offline_kit.py`.
- `compat` runs `./tests/compat-linux.sh`: the three interoperability combinations and the in-place upgrade. The script extracts the historical reference from Git into a temporary directory and removes it on exit. A shallow checkout must fetch the fixture commit named by the script first. On other systems the script reports `compat` as not run instead of passing it. The sync engine and peer run natively in Rust. The Linux desktop setup launcher also uses Python.

CI runs `rust` on Ubuntu (ext4), macOS (case-insensitive APFS) and Windows (NTFS), runs `kit` and `compat` on Ubuntu, and runs `hook` plus a Gitleaks scan of the change's commits. Hosted runners cover only their default volumes. Changes to replacement or recovery still need a run on the affected volume type, and the installed-package and SSH scenarios above remain manual.

When reporting a failed case, include the first error, peer revision and which copies survived.
