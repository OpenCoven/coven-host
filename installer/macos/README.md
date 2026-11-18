# macOS offline installer

The ZIP contains a native app. Open the extracted Coven Host app to set up the folder and open it in Finder. The installer keeps the same signed app at `~/Applications/Coven Host.app`. Open that copy after removing the download to check the local peer, synchronization and the watcher. If another app occupies that name, the installer uses `~/Applications/Coven Host Setup.app`. If setup needs attention, the window offers Copy Diagnostics, Save Diagnostics and selectable text. Reports use fixed descriptions and error classes and omit local paths, names, document contents and credentials.

The app installs its native executable under the current account’s Application Support directory. Two per-user LaunchAgents start the authenticated loopback peer and watcher at login. A new profile receives a random owner-only token, a private peer database and an ordinary Coven folder. A recognized v0.1.0 MVP installation retains its binary, port, identity, credentials, folder and jobs. An unrelated nonempty folder or an unrecognized existing profile produces a diagnostic and preserves the existing data.

## Build

Use macOS with Xcode Command Line Tools. Pass the v0.1.0 arm64 release package, an output directory, and the Intel executable compiled from the pinned Coven integration revision `40a5a18ed7abdaff6c6072695ecc0c186f4c0cd1`. The Intel executable for this release has SHA-256 `cb121c5c1952a8fb3e905e36a0844cb117b027ddc801663cbbdf30ae83940741`. The script verifies the arm64 package and qualified executable against the release hashes, verifies the Intel input, signs each helper, records the resulting hashes in the signed bundle and builds both GUI slices. The signed helper hashes differ from the input hashes because the Mach-O signature is embedded in the executable.

From the repository root, provide the full path to each build input in the shell variables below. `COVEN_SIGN_IDENTITY` selects the authorized Developer ID Application certificate’s SHA-1 fingerprint in the signing Keychain. `COVEN_NOTARY_PROFILE` names an existing Apple notarytool Keychain profile. Production signing in this repository uses Soul Protocol LLC team `9LR8Z8UQ9X`.

```sh
COVEN_SIGN_IDENTITY="$CERTIFICATE_FINGERPRINT" COVEN_NOTARY_PROFILE="$KEYCHAIN_PROFILE" \
  ./installer/macos/build.sh "$ARM_PACKAGE" "$OUTPUT" "$INTEL_BINARY" cb121c5c1952a8fb3e905e36a0844cb117b027ddc801663cbbdf30ae83940741
```

The notarized build waits for Apple, staples the ticket to the app, archives the app with its metadata, and validates the staple and Gatekeeper assessment after extracting the final ZIP. Omitting the notary profile builds a signed candidate for qualification. Omitting both signing variables creates a local ad-hoc test build. Distribution requires testing the exact final downloaded ZIP. Timestamped signatures and service-issued tickets are time dependent.

The universal GUI targets macOS 11.0 on Intel and Apple silicon. The arm64 CLI load command specifies macOS 11.0 and the Intel CLI load command specifies macOS 10.12. The combined app requires macOS 11.0 or later.

## Verify

Build the separate command-line verifier using the same installation source. The verifier accepts an extracted app and always uses a disposable profile, separate launchd labels and a private pasteboard. It has no GUI. The shipping GUI derives its profile from the current account. Python 3 and a logged-in macOS user session are needed to run these tests.

```sh
UNPACKED="$(mktemp -d)"
ditto -x -k "$OUTPUT/Coven-Host-0.1.1-macOS.zip" "$UNPACKED"
tar -xOzf "$ARM_PACKAGE" package/bin/coven > "$UNPACKED/legacy-coven"
xcrun swiftc -O -target arm64-apple-macos11.0 installer/macos/Installer.swift installer/macos/Verify.swift -o "$UNPACKED/verify-arm64"
python3 installer/macos/verify.py "$UNPACKED/Coven Host.app" "$UNPACKED/verify-arm64" --legacy-payload "$UNPACKED/legacy-coven"
xcrun swiftc -O -target x86_64-apple-macos11.0 installer/macos/Installer.swift installer/macos/Verify.swift -o "$UNPACKED/verify-intel"
python3 installer/macos/verify.py "$UNPACKED/Coven Host.app" "$UNPACKED/verify-intel"
```

The tests create the installation, observe automatic upload and download, reopen the installer, verify the same credential and service processes, and exercise a preserved nonempty folder with copyable and saved diagnostics. The arm64 fixture additionally reconstructs the earlier login services around the same peer database and checks that the old credentials and bytes still work. Intel execution on Apple silicon uses Rosetta.
