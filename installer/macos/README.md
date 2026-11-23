# macOS offline installer

The ZIP contains a native app. Open the extracted Coven Host app to set up the folder and open it in Finder. The installer keeps the same signed app at `~/Applications/Coven Host.app`. Open that copy after removing the download to check the local peer, synchronization and the watcher. If another app occupies that name, the installer uses `~/Applications/Coven Host Setup.app`. If setup needs attention, the window offers Copy Diagnostics, Save Diagnostics and selectable text. Reports use fixed descriptions and error classes and omit local paths, names, document contents and credentials.

The app installs its native executable under the current account’s Application Support directory. Two per-user LaunchAgents start the authenticated loopback peer and watcher at login. A new profile receives a random owner-only token, a private peer database and an ordinary Coven folder. For a recognized v0.1.0 or v0.1.1 installation, setup checks the saved service definitions, executable digest, folder, and peer. It replaces the login service executables while retaining the original port, peer storage, credentials, and synchronized folder. An unrelated nonempty folder or an unrecognized existing profile produces a diagnostic and preserves the existing data.

## Build

Use macOS with Xcode Command Line Tools. Build the Apple silicon and Intel binaries from the same locked Coven consumer revision and supply their independently recorded SHA-256 digests. Supply that checkout’s LICENSE file. The builder verifies both executable hashes and architectures before signing the helpers, embedding their final hashes in the signed app, and assembling both GUI slices.

From the repository root, provide the full path to each build input in the shell variables below. `COVEN_SIGN_IDENTITY` selects the authorized Developer ID Application certificate’s SHA-1 fingerprint in the signing Keychain. `COVEN_NOTARY_PROFILE` names an existing Apple notarytool Keychain profile. Production signing in this repository uses Soul Protocol LLC team `9LR8Z8UQ9X`.

```sh
COVEN_SIGN_IDENTITY="$CERTIFICATE_FINGERPRINT" COVEN_NOTARY_PROFILE="$KEYCHAIN_PROFILE" \
  ./installer/macos/build.sh "$ARM_BINARY" "$ARM_SHA256" "$COVEN_LICENSE" "$OUTPUT" "$INTEL_BINARY" "$INTEL_SHA256"
```

The notarized build waits for Apple, staples the ticket to the app, archives the app with its metadata, and validates the staple and Gatekeeper assessment after extracting the final ZIP. Omitting the notary profile builds a signed candidate for qualification. Omitting both signing variables creates a local ad-hoc test build. Distribution requires testing the exact final downloaded ZIP. Timestamped signatures and service-issued tickets are time dependent.

Build both CLI slices with `MACOSX_DEPLOYMENT_TARGET=11.0` to match the universal GUI. Inspect each final executable with `xcrun vtool -show-build` and record its minimum OS before distributing the archive.

## Verify

Build the separate command-line verifier using the same installation source. The verifier accepts an extracted app and always uses a disposable profile, separate launchd labels and a private pasteboard. It has no GUI. The shipping GUI derives its profile from the current account. Python 3 and a logged-in macOS user session are needed to run these tests.

```sh
UNPACKED="$(mktemp -d)"
ditto -x -k "$OUTPUT/Coven-Host-0.1.2-macOS.zip" "$UNPACKED"
xcrun swiftc -O -target arm64-apple-macos11.0 installer/macos/Installer.swift installer/macos/Verify.swift -o "$UNPACKED/verify-arm64"
python3 installer/macos/verify.py "$UNPACKED/Coven Host.app" "$UNPACKED/verify-arm64" --previous-app "$PREVIOUS_APP" --previous-verifier "$PREVIOUS_VERIFIER"
xcrun swiftc -O -target x86_64-apple-macos11.0 installer/macos/Installer.swift installer/macos/Verify.swift -o "$UNPACKED/verify-intel"
python3 installer/macos/verify.py "$UNPACKED/Coven Host.app" "$UNPACKED/verify-intel"
```

The tests create the installation, observe automatic upload and download, reopen the installer, verify the same credential and service processes, and exercise a preserved nonempty folder with copyable and saved diagnostics. For the upgrade check, use the downloaded v0.1.1 app and a verifier built from the v0.1.1 tag. Passing the original v0.1.0 arm64 executable with `--legacy-payload` also exercises the older login service layout. Intel execution on Apple silicon uses Rosetta.
