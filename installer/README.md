# Build and qualify the desktop installers

Start with the [release installation guide](../docs/setup.md) for user-visible behavior. Each installer sets up the existing native Rust CLI, a protected local peer and a watcher. Installer changes belong in this directory. The CLI and daemon integration used by v0.1.1 is Coven commit `40a5a18ed7abdaff6c6072695ecc0c186f4c0cd1`, with the Host library pinned at `732d8c3a06fb51fc32ade8eba46ab384c3924968`.

| Component | Source and build entry | Qualification |
| --- | --- | --- |
| macOS universal app | [macOS build and signing instructions](macos/README.md) | Fresh and repeat installs, collision handling, retained credentials, bidirectional sync, diagnostics, signed code and Gatekeeper assessment of the downloaded archive |
| Windows user installer | [Windows build instructions](windows/README.md) | Windows sign-in, peer and watcher, folder opening, token ACLs, both transfer directions, retry, Copy and Save |
| Linux desktop and portable | [Package builder](linux/build.py) and [installed test](linux/check-installed.py) | Debian package installation and desktop activation, portable setup, headless operation, user-service restart, protected existing state and both transfer directions |
| Download guide | [Start-Here.html](Start-Here.html) | Platform selection, manual override, alternative assets, policy and download failure reports, clipboard rejection and report saving |

## Linux release inputs

Use binaries from the reviewed Coven manifest and lockfile. Record the SHA-256 of each executable during its installed qualification. Supply the matching Coven checkout’s LICENSE. The builder verifies the executable hash and ELF architecture and records its glibc requirement.

```sh
python3 installer/linux/build.py --arch amd64 --native "$X64_CLI" --sha256 "$X64_SHA256" --coven-license "$COVEN_LICENSE" --out "$OUTPUT"
python3 installer/linux/build.py --arch arm64 --native "$ARM64_CLI" --sha256 "$ARM64_SHA256" --coven-license "$COVEN_LICENSE" --out "$OUTPUT"
```

## Offline kit

Put the qualified macOS ZIP, Windows EXE, two Linux desktop packages and two Linux portable archives in the output directory. With Python 3.11 or later, run `python3 installer/package-offline.py "$OUTPUT"`. The kit includes a local copy of the download guide, relative links to those assets and an internal checksum manifest. Upload the original guide separately for people who prefer downloading a single installer. Compare the published asset bytes with the signed release manifest before announcing distribution.

The packager reads each installer once and calculates its checksum from the bytes written to the ZIP. Fixed ZIP timestamps and permissions make repeat builds from identical inputs byte-for-byte reproducible. Each build uses its own temporary directory and replaces the output only after the ZIP closes successfully. Run `python3 tests/test_offline_kit.py` to check reproducibility, input replacement, overlapping builds and recovery from a failed read.
