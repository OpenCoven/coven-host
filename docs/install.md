# Advanced installation from CLI packages

For the desktop installer and automatic local folder setup, use the [installer guide](setup.md). The instructions below apply to the v0.1.0 CLI packages.

Download the CLI wrapper, the native package for your machine, and `SHA256SUMS` from the [Coven Host release](https://github.com/OpenCoven/coven-host/releases/tag/v0.1.0). Access to the private repository is required. The packages install offline with npm and Node.js 18 or later. The release uses package version `0.1.0-host.1` to distinguish it from the public Coven CLI.

| Platform | Native package | Wrapper for every platform |
| --- | --- | --- |
| Apple Silicon macOS | `opencoven-cli-macos-0.1.0-host.1.tgz` | `opencoven-cli-0.1.0-host.1.tgz` |
| Linux x64 with glibc | `opencoven-cli-linux-x64-0.1.0-host.1.tgz` | `opencoven-cli-0.1.0-host.1.tgz` |
| Windows x64 | `opencoven-cli-windows-0.1.0-host.1.tgz` | `opencoven-cli-0.1.0-host.1.tgz` |

## macOS and Linux

Run this in the download directory. Set `platform=linux-x64` on Linux. Use a dedicated installation directory and daemon profile so an existing Coven installation can keep running.

```sh
(
  set -eu
  platform=macos
  wrapper=opencoven-cli-0.1.0-host.1.tgz
  native="opencoven-cli-$platform-0.1.0-host.1.tgz"
  for file in "$wrapper" "$native"; do
    expected=$(awk -v f="$file" '$2 == f { print $1 }' SHA256SUMS)
    actual=$(shasum -a 256 "$file" | awk '{print $1}')
    test "$actual" = "$expected"
  done
  npm install --prefix "$HOME/.local/share/coven-host/v0.1.0" --offline --no-audit --no-fund --ignore-scripts "./$wrapper" "./$native"
)
```

After the installation succeeds, set these variables in each terminal where you use this installation.

```sh
export PATH="$HOME/.local/share/coven-host/v0.1.0/node_modules/.bin:$PATH"
export COVEN_HOME="$HOME/.local/state/coven-host/profile"
coven host init --root "$HOME/Coven"
coven host status
```

## Windows PowerShell

Run this in the download directory. It verifies the selected files and installs them under the current account.

```powershell
$wrapper = 'opencoven-cli-0.1.0-host.1.tgz'
$native = 'opencoven-cli-windows-0.1.0-host.1.tgz'
foreach ($file in @($wrapper, $native)) {
    $entry = @(Get-Content SHA256SUMS | Where-Object { ($_ -split '\s+', 2)[1] -eq $file })
    if ($entry.Count -ne 1 -or (Get-FileHash $file -Algorithm SHA256).Hash -ne ($entry[0] -split '\s+')[0]) { throw "Checksum mismatch for $file" }
}
$prefix = Join-Path $env:LOCALAPPDATA 'CovenHost\v0.1.0'
npm.cmd install --prefix $prefix --offline --no-audit --no-fund --ignore-scripts ".\$wrapper" ".\$native"
if ($LASTEXITCODE) { throw "npm install failed" }
$env:COVEN_HOME = Join-Path $env:LOCALAPPDATA 'CovenHost\profile'
$coven = Join-Path $prefix 'node_modules\.bin\coven.cmd'
& $coven host init --root (Join-Path $HOME 'Coven')
& $coven host status
```

## Connect the directory

Initialize one folder per client profile. Start the peer with a protected token and persistent storage. Use a loopback URL for a peer on the same computer, or use a verified SSH tunnel to a separate machine. Run `coven host watch` for continuous sync. The [operator guide](https://github.com/OpenCoven/coven-host/blob/v0.1.0/docs/operations.md) covers token setup, peer startup, connection and conflict recovery.

To check the result, save a file under `Castles/demo/files/`, synchronize, and read it with `coven host get`. Use `coven host demo-result` on the peer and check for `Castles/demo/results/hello-from-host.md` in the folder. A peer on the same physical disk provides local synchronization. Keep a separate backup for disk failure.

## Build provenance

The packages contain the Coven CLI and daemon from `40a5a18ed7abdaff6c6072695ecc0c186f4c0cd1` with the Coven Host Rust dependency pinned to `732d8c3a06fb51fc32ade8eba46ab384c3924968`. The release includes the signed integration commits in `coven-cli-host-source.bundle`. To inspect or rebuild them, clone OpenCoven/coven, then run these commands in that checkout with the downloaded bundle’s absolute path. Building requires read access to the private Coven Host dependency.

```sh
git bundle verify /path/to/coven-cli-host-source.bundle
git fetch /path/to/coven-cli-host-source.bundle refs/heads/feat/1805-native-host
git switch --detach 40a5a18ed7abdaff6c6072695ecc0c186f4c0cd1
cargo build --locked --release -p coven-cli
```
