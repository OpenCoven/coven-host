# Install Coven Host

Download the installer for the computer where you want the folder. Open it in your usual account. It sets up a private peer on that computer, starts synchronization, and opens your Coven folder. [Coven Host downloads](https://github.com/OpenCoven/coven-host/releases/tag/v0.1.2)

| Computer | Download | What to do |
| --- | --- | --- |
| Mac, Apple Silicon or Intel | [macOS installer](https://github.com/OpenCoven/coven-host/releases/download/v0.1.2/Coven-Host-0.1.2-macOS.zip) | Open the ZIP, then open the Coven Host app. |
| Windows x64, including Windows 11 on ARM with x64 emulation | [Windows setup](https://github.com/OpenCoven/coven-host/releases/download/v0.1.2/Coven-Host-0.1.2-Windows-x64.exe) | Open the downloaded EXE. Use **Coven Host** in the Start menu on subsequent visits. |
| Ubuntu or Debian, Intel/AMD 64-bit | [Linux desktop package](https://github.com/OpenCoven/coven-host/releases/download/v0.1.2/Coven-Host-0.1.2-linux-amd64.deb) | Open the package, select **Install**, then open **Coven Host** from Applications. |
| Ubuntu or Debian, ARM64 | [ARM64 desktop package](https://github.com/OpenCoven/coven-host/releases/download/v0.1.2/Coven-Host-0.1.2-linux-arm64.deb) | Open the package, select **Install**, then open **Coven Host** from Applications. |

If you are unsure which download to choose, open the [download guide](https://github.com/OpenCoven/coven-host/releases/download/v0.1.2/Start-Here.html). For offline installation, download the [offline kit](https://github.com/OpenCoven/coven-host/releases/download/v0.1.2/Coven-Host-0.1.2-Offline.zip), extract the entire ZIP, and open **Start-Here.html**. It selects a platform when your browser identifies one and lets you change the selection.

## Use the folder

Save a document under `Coven/Castles/demo/files/`. Results from the peer appear under `Coven/Castles/demo/results/`. The local peer and watcher resume when you sign in. Both copies live on the same computer. To use a peer on a different computer, follow the [operator guide](operations.md).

If two edits conflict, keep both versions and choose how to resolve them. A deletion stays visible for resolution. Reopening the installer checks the existing setup. If it finds an unrecognized profile, follow the displayed recovery guidance before moving files or changing its connection.

## When a package cannot open

- **Linux without a graphical package manager or administrator access.** Download the [x86-64 portable archive](https://github.com/OpenCoven/coven-host/releases/download/v0.1.2/Coven-Host-0.1.2-linux-x86_64.tar.gz) or [ARM64 portable archive](https://github.com/OpenCoven/coven-host/releases/download/v0.1.2/Coven-Host-0.1.2-linux-aarch64.tar.gz). Extract it and open **Install Coven Host**. On a server, run `./'Install Coven Host'` from the extracted directory. The Linux launcher requires Python 3.8 or later and glibc 2.34 or later. The native sync engine is Rust. Without a systemd user manager, use desktop autostart or run the launcher with `--foreground` under your service manager.
- **Windows publisher warning or managed-device policy.** Windows may display an unknown-publisher warning. Use the signed release checksum manifest to verify the download and ask your administrator to approve the installer on managed devices.
- **Older or different devices.** The macOS installer requires macOS 11 or later. Windows requires x64 execution. The Linux downloads require x86-64 or ARM64 with glibc. For other desktop configurations, the [advanced installation and source instructions](install.md) describe the existing CLI packages and the source needed for a port. Phones, tablets and 32-bit systems do not have a qualified installer.
- **Download failure.** Open the release page in your browser and choose the asset for your computer. For a machine without internet access, transfer the offline kit and extract it before opening the guide.

## Copy a diagnostic report

Reopen Coven Host if setup or synchronization needs attention. The error explains the failed operation and a next step. Use **Copy Diagnostics** or **Copy report** where available. If the clipboard is blocked, select the report text or save the report. On Linux, the launcher also prints diagnostics in a terminal and normally saves its latest error in `~/.local/state/coven-host-support/last-error.txt`.

If the operating system blocks the app before it can run, open **Trouble downloading or opening the installer?** in the download guide or offline kit. Choose the symptom, then copy or save the report. This report identifies the symptom you selected. Reports created by the running installer include its observations. Review the report before sharing it. Include its code, stage, UTC time, release, platform, expected result, observed result and reproduction step. A useful follow-up is whether the exact same stage failed on retry.
