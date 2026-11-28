# Windows installer

Open the Windows x64 EXE from the private release. Windows 11 on ARM64 can run its x64 CLI through the operating system's emulation. The native Windows Forms launcher uses the .NET Framework included with Windows. Setup runs under the signed-in account, verifies and installs the bundled CLI, and starts the folder watcher. **Open folder** opens the active folder in Explorer. On a new setup the synchronized directories are `%USERPROFILE%\Coven\Castles\demo\files` and `results`.

Open **Coven Host** in Start to check sync or restart stopped processes. The application reuses the configured folder, peer, access token and database. A fresh configuration uses a private peer on an available IPv4 loopback port. To synchronize with another machine, follow the [SSH instructions](../../docs/operations.md#connect-through-ssh) with the installed CLI. A subsequent launch recognizes an existing configured remote. Setup registers a sign-in entry named **OpenCovenHost**. Windows Startup Apps settings control whether it runs at login.

If setup reports CH-W12, Windows is creating files with a group as their owner. Open the installer from File Explorer in your regular signed-in session so that your account owns its private recovery files.

If setup reports CH-W11 after updating from v0.1.1, save your work, sign out of Windows, and sign back in. The registered launcher reopens the original folder and peer with the new executable.

The versioned executables and license texts live in `%LOCALAPPDATA%\OpenCoven\Coven Host\0.1.2`. The sibling `local-peer` directory holds the token, port, process records and peer database. The token inherits an ACL granting access only to the current user. The production daemon manages its existing profile, folder state and peer identity pin. Keep backups of the peer and client state with their respective processes stopped, as described in the [operator guide](../../docs/operations.md).

If setup reports a problem, follow the next step in the window. **Copy report** copies the displayed diagnostic. **Save report** exports it to a text file. You can also select the text and press Ctrl+C. **Retry** checks the same installation again. Diagnostic reports include the release, UTC time, Windows build, native architecture, stage, stable code and a bounded observation. The reporter excludes command output, remote addresses, account and folder names, document contents, and token values. Windows security prompts that appear before the application opens are controlled by Windows.

## Build

Use a Windows 11 build machine with its inbox Windows PowerShell, .NET Framework 4.x compiler and tar.exe. Supply the Windows x64 CLI package built from the reviewed consumer SHA. Take the archive and executable SHA-256 values from its installed qualification receipt. Use the LICENSE file from the same Coven source revision. In a PowerShell session authorized to run your reviewed build sources, run the build from this directory.

```powershell
.\build.ps1 -Package $Package -OutputDirectory $OutputDirectory -CovenLicense $ConsumerLicense -PackageSha256 $ArchiveHash -CliSha256 $ExecutableHash
```

The build verifies both the source archive and the extracted executable against the supplied qualification digests. It embeds the compressed executable and both licenses in `Coven-Host-0.1.2-Windows-x64.exe`. The installer verifies the executable digest before installing it and at setup or sign-in. The .NET Framework compiler adds build-specific PE metadata. Record the SHA-256 of the final distributable after any authorized signing. The build command produces an unsigned EXE.

If Windows or organizational policy blocks the download or launch, record the message and use the release's support instructions. Organizations requiring an Authenticode publisher signature need a publisher-signed Windows release.

## Repeat installed acceptance

Use a disposable Windows 11 account on NTFS. Start with an empty Coven Host profile, open the downloaded EXE, wait for the ready window and choose **Open folder**. Create a small text file under `Castles\demo\files`. Read it through the installed CLI with `host get`, then use `host put` to publish a different document under `results/`. Compare the exact bytes in both directions while the GUI is closed. Supply the saved loopback port and the protected token file reference to those commands.

For recovery, stop only the disposable peer, edit a document and confirm that a sync attempt reports the peer is offline. Restart Windows and sign in normally. Use `host get` to confirm that the queued edit reached the original peer automatically. When checking startup issues, inspect the recorded port and the peer and watch process identities. A pre-reboot daemon process ID can belong to a protected Windows process after reboot. If the recorded daemon creation time predates this boot, setup can start the production daemon through its exclusive serve lock and owner-only pipe.

To exercise a setup failure, temporarily move the disposable peer token to a protected sibling location and reopen **Coven Host**. Verify that the diagnostic includes CH-W05, the release version, Windows build, native architecture and a recovery step. Compare the selected text with **Copy report** and **Save report**. Restore the original token and select **Retry**. Confirm that the ready window returns and both the existing peer and watcher are reused. To check remote configuration preservation, configure another endpoint in the disposable CLI, reopen **Coven Host**, and compare the exact remote value in `host status`.
