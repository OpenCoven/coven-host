#!/bin/bash
set -euo pipefail
if [[ $# != 4 && $# != 6 ]]; then
  echo "Usage: build.sh arm64-executable expected-arm64-sha256 coven-license output-directory [intel-executable expected-intel-sha256]" >&2
  exit 2
fi
src="$(cd "$(dirname "$0")" && pwd)"
out="$4"
mkdir -p "$out"
out="$(cd "$out" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
app="$work/Coven Host.app"
res="$app/Contents/Resources"
mac="$app/Contents/MacOS"
helpers="$app/Contents/Helpers"
mkdir -p "$res" "$mac" "$helpers"
check() {
  [[ "$(shasum -a 256 "$1" | cut -d' ' -f1)" == "$2" ]] || { echo "Payload integrity check failed" >&2; exit 1; }
}
check "$1" "$2"
/usr/bin/lipo -verify_arch arm64 "$1"
cp "$1" "$helpers/coven-arm64"
cp "$3" "$res/LICENSE"
cp "$src/../../LICENSE" "$res/LICENSE-COVEN-HOST"
p="$app/Contents/Info.plist"
cp "$src/Info.plist" "$p"
archs=(arm64)
if [[ $# == 6 ]]; then
  check "$5" "$6"
  /usr/bin/lipo -verify_arch x86_64 "$5"
  cp "$5" "$helpers/coven-x86_64"
  archs+=(x86_64)
fi
outputs=()
for arch in "${archs[@]}"; do
  native="$helpers/coven-$arch"
  chmod 0755 "$native"
  if [[ -n "${COVEN_SIGN_IDENTITY:-}" ]]; then
    codesign --force --options runtime --timestamp --sign "$COVEN_SIGN_IDENTITY" --identifier "org.opencoven.coven-host.cli.$arch" "$native"
    codesign --verify -R '=anchor apple generic and certificate leaf[subject.OU] = "9LR8Z8UQ9X"' "$native"
  else
    codesign --force --sign - --identifier "org.opencoven.coven-host.cli.$arch" "$native"
  fi
  shipped="$(shasum -a 256 "$native" | cut -d' ' -f1)"
  /usr/libexec/PlistBuddy -c "Add :CovenPayloadSHA-$arch string $shipped" "$p"
  xcrun swiftc -O -target "${arch}-apple-macos11.0" -module-name CovenHostSetup -o "$work/setup-$arch" "$src/Installer.swift" "$src/main.swift"
  outputs+=("$work/setup-$arch")
done
xcrun lipo -create "${outputs[@]}" -output "$mac/CovenHostSetup"
if [[ -n "${COVEN_SIGN_IDENTITY:-}" ]]; then
  codesign --force --options runtime --timestamp --sign "$COVEN_SIGN_IDENTITY" "$app"
  codesign --verify -R '=anchor apple generic and certificate leaf[subject.OU] = "9LR8Z8UQ9X"' "$app"
else
  codesign --force --sign - --identifier org.opencoven.coven-host.installer "$app"
fi
codesign --verify --deep --strict --verbose=2 "$app"
ditto -c -k --keepParent --norsrc "$app" "$work/release.zip"
if [[ -n "${COVEN_NOTARY_PROFILE:-}" ]]; then
  : "${COVEN_SIGN_IDENTITY:?Notarization requires a Developer ID signature}"
  xcrun notarytool submit "$work/release.zip" --keychain-profile "$COVEN_NOTARY_PROFILE" --wait
  xcrun stapler staple "$app"
  xcrun stapler validate "$app"
  spctl -a -t exec -vv "$app"
  rm "$work/release.zip"
  ditto -c -k --keepParent --sequesterRsrc "$app" "$work/release.zip"
  ditto -x -k "$work/release.zip" "$work/reopened"
  xcrun stapler validate "$work/reopened/Coven Host.app"
  spctl -a -t exec -vv "$work/reopened/Coven Host.app"
fi
mv "$work/release.zip" "$out/Coven-Host-0.1.2-macOS.zip"
shasum -a 256 "$out/Coven-Host-0.1.2-macOS.zip"
