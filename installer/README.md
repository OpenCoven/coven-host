# Build and qualify the desktop installers

Start with the [release installation guide](../docs/setup.md) for user-visible behavior. Each installer sets up the existing native Rust CLI, a protected local peer and a watcher. Installer changes belong in this directory. The CLI and daemon integration used by v0.1.1 is Coven commit `40a5a18ed7abdaff6c6072695ecc0c186f4c0cd1`, with the Host library pinned at `732d8c3a06fb51fc32ade8eba46ab384c3924968`.

| Component | Source and build entry | Qualification |
| --- | --- | --- |
| macOS universal app | [macOS build and signing instructions](macos/README.md) | Fresh and repeat installs, collision handling, retained credentials, bidirectional sync, diagnostics, signed code and Gatekeeper assessment of the downloaded archive |
| Windows user installer | [Windows build instructions](windows/README.md) | Windows sign-in, peer and watcher, folder opening, token ACLs, both transfer directions, retry, Copy and Save |
| Linux desktop and portable | [Package builder](linux/build.py) and [installed test](linux/check-installed.py) | Debian package installation and desktop activation, portable setup, headless operation, user-service restart, protected existing state and both transfer directions |
| Download guide | [Start-Here.html](Start-Here.html) | Platform selection, manual override, alternative assets, policy and download failure reports, clipboard rejection and report saving |

## Linux release inputs

Use the qualified Linux x64 CLI tarball from the [v0.1.0 release](https://github.com/OpenCoven/coven-host/releases/tag/v0.1.0). Build the ARM64 CLI from the same locked integration revision using the pinned Rust toolchain on Linux ARM64. Verify its executable digest against the qualified build before packaging it. Both builders take their license files from the CLI package.

```sh
python3 installer/linux/build.py --package "$LINUX_CLI_TARBALL" --out "$OUTPUT"
python3 installer/linux/build.py --arch arm64 --native "$ARM64_CLI" --sha256 1cc0910d8f5210f0696fdf01c372f74bf93eac43c748f1babaadb6b62410d013 --package "$LINUX_CLI_TARBALL" --out "$OUTPUT"
```

## Offline kit

Put the qualified macOS ZIP, Windows EXE, two Linux desktop packages and two Linux portable archives in the output directory. With Python 3.11 or later, run `python3 installer/package-offline.py "$OUTPUT"`. The kit includes a local copy of the download guide, relative links to those assets and an internal checksum manifest. Upload the original guide separately for people who prefer downloading a single installer. Compare the published asset bytes with the signed release manifest before announcing distribution.
