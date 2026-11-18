using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Globalization;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Management;
using System.Net;
using System.Net.Sockets;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Security.AccessControl;
using System.Security.Cryptography;
using System.Security.Principal;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using System.Web.Script.Serialization;
using System.Windows.Forms;
using Microsoft.Win32;

[assembly: AssemblyTitle("Coven Host")]
[assembly: AssemblyCompany("OpenCoven")]
[assembly: AssemblyVersion("0.1.1.0")]
[assembly: AssemblyFileVersion("0.1.1.0")]

internal sealed class SetupFailure : Exception
{
    internal readonly string Code, Stage, Next;
    internal SetupFailure(string code, string stage, string observation, string next) : base(observation)
    { Code = code; Stage = stage; Next = next; }
}

internal sealed class Setup
{
    internal const string Version = "0.1.1";
    const string CliHash = "1e7a6e20030d01492e60607ec7b9916a26b4bdbc4ccc2c1594877fd50c5786cf";
    readonly string app = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "OpenCoven", "Coven Host");
    internal string Root = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), "Coven");
    internal string Report { get; private set; }
    internal string Note { get; private set; }
    string Bin { get { return Path.Combine(app, Version); } }
    string Launcher { get { return Path.Combine(Bin, "Coven Host.exe"); } }
    string Cli { get { return Path.Combine(Bin, "coven.exe"); } }
    string State { get { return Path.Combine(app, "local-peer"); } }
    string Token { get { return Path.Combine(State, "access.token"); } }
    string Server { get { return Path.Combine(State, "server"); } }
    string PortFile { get { return Path.Combine(State, "port"); } }
    string stage, code, next;
    readonly Action<string> progress;
    internal Setup(Action<string> onProgress) { progress = onProgress; }

    void Step(string id, string name, string action)
    {
        code = id; stage = name; next = action;
        progress(name);
    }

    void Fail(string observation) { throw new SetupFailure(code, stage, observation, next); }

    internal bool Run(bool resume)
    {
        Step("CH-W01", "Open setup", "Close another Coven Host setup window, then select Retry.");
        try
        {
            using (var guard = new Mutex(false, @"Local\OpenCoven.Host.Setup." + WindowsIdentity.GetCurrent().User.Value))
            {
                bool held;
                try { held = guard.WaitOne(TimeSpan.FromSeconds(35)); }
                catch (AbandonedMutexException) { held = true; }
                if (!held) Fail("Another setup operation is still running.");
                try { Configure(resume); } finally { guard.ReleaseMutex(); }
            }
            return true;
        }
        catch (Exception e)
        {
            var known = e as SetupFailure;
            // Raw exception messages and CLI output can contain credentials, folder names and remote URLs.
            string observation = known == null ? e.GetType().Name + " (HRESULT 0x" + e.HResult.ToString("X8") + ")" : known.Message;
            Report = "Coven Host setup diagnostic" + Environment.NewLine
                + "Code: " + code + Environment.NewLine + "Stage: " + stage + Environment.NewLine
                + "UTC: " + DateTime.UtcNow.ToString("o") + Environment.NewLine + "Version: " + Version + Environment.NewLine
                + "OS: Windows " + Environment.OSVersion.Version + Environment.NewLine + "Architecture: " + Architecture() + Environment.NewLine
                + "Observed: " + observation + Environment.NewLine + "Reproduce: Open Coven Host from Start or run the installer again."
                + Environment.NewLine + "Next step: " + next;
            return false;
        }
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool IsWow64Process2(IntPtr process, out ushort machine, out ushort nativeMachine);
    static string Architecture()
    {
        ushort machine, native;
        using (var self = Process.GetCurrentProcess())
            if (IsWow64Process2(self.Handle, out machine, out native))
                return native == 0xaa64 ? "ARM64" : native == 0x8664 ? "x64" : native == 0x014c ? "x86" : "unknown";
        return "unknown";
    }

    static void Ordinary(string path)
    {
        for (string p = Path.GetFullPath(path); p != null; p = Path.GetDirectoryName(p))
        {
            try
            {
                if ((File.GetAttributes(p) & FileAttributes.ReparsePoint) != 0) throw new IOException("Reparse point");
            }
            catch (FileNotFoundException) { }
            catch (DirectoryNotFoundException) { }
        }
    }

    static string Hash(string path)
    {
        using (var file = File.OpenRead(path))
        using (var sha = SHA256.Create())
            return BitConverter.ToString(sha.ComputeHash(file)).Replace("-", "").ToLowerInvariant();
    }

    static void Atomic(string path, Action<string> write, bool replace)
    {
        Ordinary(path);
        string temp = Path.Combine(Path.GetDirectoryName(path), Guid.NewGuid().ToString("N") + ".tmp");
        try
        {
            write(temp);
            if (replace && File.Exists(path)) File.Replace(temp, path, null);
            else File.Move(temp, path);
        }
        finally { if (File.Exists(temp)) File.Delete(temp); }
    }

    static void PrivateDirectory(string dir)
    {
        Ordinary(dir);
        var security = new DirectorySecurity();
        var owner = WindowsIdentity.GetCurrent().User;
        security.SetOwner(owner);
        security.SetAccessRuleProtection(true, false);
        security.AddAccessRule(new FileSystemAccessRule(owner, FileSystemRights.FullControl,
            InheritanceFlags.ContainerInherit | InheritanceFlags.ObjectInherit, PropagationFlags.None, AccessControlType.Allow));
        Directory.CreateDirectory(dir, security);
        // Existing private application directories may have been created by an interrupted install.
        Directory.SetAccessControl(dir, security);
    }

    static void Resource(string name, string target)
    {
        if (File.Exists(target)) return;
        Atomic(target, delegate(string temp) {
            using (var src = Assembly.GetExecutingAssembly().GetManifestResourceStream(name))
            using (var dst = File.Create(temp)) src.CopyTo(dst);
        }, false);
    }

    void Install()
    {
        Step("CH-W02", "Install application", "Check that your Windows account can write to Local AppData and has free disk space, then select Retry.");
        if (Architecture() != "x64" && Architecture() != "ARM64")
            Fail("The bundled CLI requires a Windows x64 system or Windows 11 on ARM64 with x64 app support.");
        PrivateDirectory(app);
        Ordinary(Bin); Directory.CreateDirectory(Bin);
        Ordinary(Cli);
        if (!File.Exists(Cli))
        {
            Atomic(Cli, delegate(string temp) {
                using (var src = Assembly.GetExecutingAssembly().GetManifestResourceStream("cli.gz"))
                using (var zip = new GZipStream(src, CompressionMode.Decompress))
                using (var dst = File.Create(temp)) zip.CopyTo(dst);
                if (Hash(temp) != CliHash) Fail("The embedded CLI did not match its release hash.");
            }, false);
        }
        if (Hash(Cli) != CliHash)
            Fail("The installed CLI has an unexpected SHA-256. Move the versioned application directory aside after closing its processes, then reopen the installer.");
        string self = Assembly.GetExecutingAssembly().Location;
        Ordinary(Launcher);
        if (!File.Exists(Launcher)) Atomic(Launcher, t => File.Copy(self, t), false);
        else if (!String.Equals(Path.GetFullPath(self), Launcher, StringComparison.OrdinalIgnoreCase) && Hash(self) != Hash(Launcher))
            Fail("The installed launcher differs from this version. Obtain the release asset and verify its checksum before replacing the versioned application directory.");
        Resource("license-cli.txt", Path.Combine(Bin, "LICENSE-CLI.txt"));
        Resource("license-host.txt", Path.Combine(Bin, "LICENSE-HOST.txt"));
    }

    void Register()
    {
        Step("CH-W07", "Register sign-in startup", "Check that your account can create Start menu shortcuts and sign-in applications, then select Retry.");
        string programs = Environment.GetFolderPath(Environment.SpecialFolder.Programs);
        Directory.CreateDirectory(programs);
        dynamic shell = Activator.CreateInstance(Type.GetTypeFromProgID("WScript.Shell", true));
        try
        {
            dynamic shortcut = shell.CreateShortcut(Path.Combine(programs, "Coven Host.lnk"));
            try
            {
                shortcut.TargetPath = Launcher;
                shortcut.WorkingDirectory = Bin;
                shortcut.Description = "Open your Coven folder and check synchronization.";
                shortcut.Save();
            }
            finally { Marshal.FinalReleaseComObject(shortcut); }
        }
        finally { Marshal.FinalReleaseComObject(shell); }
        using (var run = Registry.CurrentUser.CreateSubKey(@"Software\Microsoft\Windows\CurrentVersion\Run"))
            run.SetValue("OpenCovenHost", Quote(Launcher) + " --resume", RegistryValueKind.String);
    }

    // All arguments are application literals or absolute file paths without trailing separators.
    static string Quote(string arg) { return "\"" + arg + "\""; }
    ProcessStartInfo Command(string[] args, bool capture)
    {
        return new ProcessStartInfo(Cli, String.Join(" ", args.Select(Quote))) {
            UseShellExecute = false, CreateNoWindow = true, WorkingDirectory = Bin,
            RedirectStandardOutput = capture, RedirectStandardError = capture
        };
    }

    sealed class Result { internal int Exit; internal string Output, Error; }
    Result RunCli(params string[] args)
    {
        using (var process = Process.Start(Command(args, true)))
        {
            var stdout = process.StandardOutput.ReadToEndAsync();
            var stderr = process.StandardError.ReadToEndAsync();
            if (!process.WaitForExit(45000))
            {
                try { process.Kill(); } catch (InvalidOperationException) { }
                Fail("The installed CLI did not finish within 45 seconds.");
            }
            if (!Task.WaitAll(new Task[] {stdout, stderr}, 3000))
                Fail("The installed CLI did not finish returning its result.");
            return new Result { Exit = process.ExitCode, Output = stdout.Result, Error = stderr.Result };
        }
    }

    void Success(Result r)
    {
        if (r.Exit == 0) return;
        string detail = "The installed CLI exited with code " + r.Exit.ToString(CultureInfo.InvariantCulture) + ".";
        string message = r.Error.ToLowerInvariant();
        if (message.Contains("offline") || message.Contains("connection refused") || message.Contains("actively refused")) detail += " The peer is offline or refused the connection.";
        else if (message.Contains("conflict")) detail += " The folder has an unresolved conflict.";
        else if (message.Contains("capability") || message.Contains("hostsync")) detail += " The running daemon may need the hostSync capability.";
        else if (message.Contains("daemon pid")) detail += " The existing daemon process identity could not be verified.";
        else if (message.Contains("identity") || message.Contains("rollback")) detail += " The peer identity or retained revision history needs inspection.";
        else if (message.Contains("permission") || message.Contains("access is denied") || message.Contains("dacl")) detail += " Windows denied access or a private ACL check failed.";
        Fail(detail);
    }

    static string DaemonRecord()
    {
        string profile = Environment.GetEnvironmentVariable("COVEN_HOME");
        if (String.IsNullOrEmpty(profile))
        {
            string home = Environment.GetEnvironmentVariable("HOME");
            if (String.IsNullOrEmpty(home)) home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
            profile = Path.Combine(home, ".coven");
        }
        return Path.Combine(profile, "daemon.json");
    }

    bool PriorBootRecord(string path, string error)
    {
        Ordinary(path);
        if (!File.Exists(path)) return false;
        var record = new JavaScriptSerializer().Deserialize<Dictionary<string, object>>(File.ReadAllText(path));
        object stamp, id;
        if (!record.TryGetValue("processCreationTime", out stamp) || !record.TryGetValue("pid", out id)) return false;
        long created;
        if (!Int64.TryParse(Convert.ToString(stamp, CultureInfo.InvariantCulture), out created) || created <= 0) return false;
        if (!error.Contains("could not determine whether Coven daemon pid " + id + " is alive with the recorded process identity")) return false;
        using (var query = new ManagementObjectSearcher("SELECT LastBootUpTime FROM Win32_OperatingSystem"))
        using (var rows = query.Get())
            foreach (ManagementObject row in rows)
                using (row)
                    return DateTime.FromFileTimeUtc(created) < ManagementDateTimeConverter.ToDateTime((string)row["LastBootUpTime"]).ToUniversalTime();
        return false;
    }

    Result ReadDaemonStatus()
    {
        var result = RunCli("host", "status");
        if (result.Exit == 0 || !result.Error.Contains("not clearing daemon status")) return result;
        string path = DaemonRecord();
        if (!PriorBootRecord(path, result.Error)) return result;
        // Windows may reuse a dead daemon's PID for an inaccessible system process after reboot.
        // The production serve entry point takes its exclusive lifetime lock and binds its owner-only
        // pipe before publishing fresh status. Never signal a reused PID or remove lifecycle files.
        using (var daemon = Process.Start(Command(new string[] { "daemon", "serve" }, false)))
        {
            for (int i = 0; i < 100; i++)
            {
                if (daemon.WaitForExit(100)) break;
                var record = new JavaScriptSerializer().Deserialize<Dictionary<string, object>>(File.ReadAllText(path));
                object pid;
                if (record.TryGetValue("pid", out pid) && Convert.ToInt32(pid) == daemon.Id) break;
            }
        }
        return RunCli("host", "status");
    }

    Dictionary<string, object> Status()
    {
        var result = ReadDaemonStatus();
        if (result.Exit != 0 && result.Error.Contains("client is not initialized")) return null;
        Success(result);
        var status = new JavaScriptSerializer { MaxJsonLength = 32 * 1024 * 1024 }.Deserialize<Dictionary<string, object>>(result.Output);
        if (status == null || !status.ContainsKey("remote") || !(status["root"] is string))
            Fail("The installed CLI returned an unrecognized status document.");
        return status;
    }

    void CreatePeerSettings()
    {
        PrivateDirectory(State);
        Ordinary(Token); Ordinary(PortFile); Ordinary(Server);
        if (!File.Exists(Token))
        {
            if (File.Exists(PortFile) || Directory.Exists(Server))
                Fail("An existing peer is missing its access token. Restore the original token from a protected backup.");
            var bytes = new byte[32];
            using (var rng = RandomNumberGenerator.Create()) rng.GetBytes(bytes);
            Atomic(Token, t => File.WriteAllText(t, BitConverter.ToString(bytes).Replace("-", ""), Encoding.ASCII), false);
        }
        if (!File.Exists(PortFile))
        {
            if (Directory.Exists(Server)) Fail("An existing peer is missing its port setting. Restore its original port file from backup.");
            var listener = new TcpListener(IPAddress.Loopback, 0);
            try
            {
                listener.Start();
                var port = ((IPEndPoint)listener.LocalEndpoint).Port;
                Atomic(PortFile, t => File.WriteAllText(t, port.ToString(CultureInfo.InvariantCulture), Encoding.ASCII), false);
            }
            finally { listener.Stop(); }
        }
    }

    int Port()
    {
        Ordinary(PortFile);
        int port;
        if (!Int32.TryParse(File.ReadAllText(PortFile), NumberStyles.None, CultureInfo.InvariantCulture, out port) || port < 1 || port > 65535)
            Fail("The local peer port setting is invalid. Restore its original port file from backup.");
        return port;
    }

    string LocalUrl(int port) { return "http://127.0.0.1:" + port.ToString(CultureInfo.InvariantCulture); }

    // Check the owning PID before allowing a credential-bearing first connection.
    [DllImport("iphlpapi.dll")] static extern uint GetExtendedTcpTable(IntPtr table, ref int size, bool sort, int family, int kind, int reserved);
    static int ListenerPid(int port)
    {
        int size = 0;
        GetExtendedTcpTable(IntPtr.Zero, ref size, false, 2, 3, 0);
        for (int attempt = 0; attempt < 4 && size > 0; attempt++)
        {
            var table = Marshal.AllocHGlobal(size);
            try
            {
                uint error = GetExtendedTcpTable(table, ref size, false, 2, 3, 0);
                if (error == 122) continue;
                if (error != 0) throw new IOException("Cannot inspect TCP listeners");
                int count = Marshal.ReadInt32(table);
                for (int i = 0; i < count; i++)
                {
                    var row = IntPtr.Add(table, 4 + 24 * i);
                    int address = Marshal.ReadInt32(row, 4);
                    int value = (Marshal.ReadByte(row, 8) << 8) | Marshal.ReadByte(row, 9);
                    if (value == port && (address == 0x0100007f || address == 0)) return Marshal.ReadInt32(row, 20);
                }
                return 0;
            }
            finally { Marshal.FreeHGlobal(table); }
        }
        throw new IOException("Cannot size TCP listener table");
    }

    Process Recorded(string role)
    {
        var path = Path.Combine(State, role + ".pid");
        Ordinary(path);
        if (!File.Exists(path)) return null;
        string[] fields = File.ReadAllText(path).Split(',');
        int pid; long started;
        if (fields.Length != 2 || !Int32.TryParse(fields[0], out pid) || !Int64.TryParse(fields[1], out started)) return null;
        Process p;
        try { p = Process.GetProcessById(pid); } catch (ArgumentException) { return null; }
        try
        {
            if (p.StartTime.ToUniversalTime().Ticks == started && String.Equals(p.MainModule.FileName, Cli, StringComparison.OrdinalIgnoreCase)) return p;
        }
        catch (InvalidOperationException) { }
        catch (System.ComponentModel.Win32Exception) { }
        p.Dispose();
        return null;
    }

    Process Spawn(string role, params string[] args)
    {
        var process = Process.Start(Command(args, false));
        try
        {
            var receipt = process.Id.ToString() + "," + process.StartTime.ToUniversalTime().Ticks.ToString(CultureInfo.InvariantCulture);
            Atomic(Path.Combine(State, role + ".pid"), t => File.WriteAllText(t, receipt, Encoding.ASCII), true);
            return process;
        }
        catch
        {
            if (!process.HasExited) process.Kill();
            process.Dispose(); throw;
        }
    }

    void EnsurePeer(int port, bool configured)
    {
        Step("CH-W05", "Start local peer", "Restore the existing peer database or token if missing. Close any application using the saved local port, then select Retry.");
        Ordinary(Token); Ordinary(Server); Ordinary(Path.Combine(Server, "host.sqlite"));
        if (configured && (!File.Exists(Token) || !File.Exists(Path.Combine(Server, "host.sqlite"))))
            Fail("The configured local peer is missing its token or database.");
        using (var old = Recorded("peer"))
            if (old != null && ListenerPid(port) == old.Id) return;
        if (ListenerPid(port) != 0) Fail("Another process owns the saved local port. The setup could not start its peer.");
        using (var peer = Spawn("peer", "host", "serve", "--port", port.ToString(CultureInfo.InvariantCulture), "--storage", Server, "--token-file", Token))
        {
            for (int i = 0; i < 100; i++)
            {
                if (peer.HasExited) Fail("The local peer exited during startup with code " + peer.ExitCode + ".");
                if (ListenerPid(port) == peer.Id) return;
                Thread.Sleep(100);
            }
            Fail("The local peer did not start listening within 10 seconds.");
        }
    }

    void EnsureWatch()
    {
        Step("CH-W08", "Start folder watching", "Open Coven Host again. If the error returns, restart your Windows session and select Retry.");
        using (var old = Recorded("watch")) if (old != null) return;
        using (var watch = Spawn("watch", "host", "watch"))
            if (watch.WaitForExit(1200)) Fail("The folder watcher exited during startup with code " + watch.ExitCode + ".");
    }

    void Configure(bool resume)
    {
        if (!resume) Install();
        else
        {
            Step("CH-W02", "Verify installed application", "Run the release installer again to restore a missing CLI. If an installed binary was modified, verify the release checksum before replacing it.");
            if (!File.Exists(Cli) || Hash(Cli) != CliHash) Fail("The installed CLI is missing or damaged.");
        }
        Step("CH-W03", "Check folder configuration", "Open your existing Coven application and check daemon availability. Verify that your configured folder and state are accessible, then select Retry.");
        var status = Status();
        if (status == null)
        {
            Step("CH-W04", "Initialize Coven folder", "Check that your profile is on a writable local volume and the Coven folder is accessible, then select Retry.");
            Success(RunCli("host", "init", "--root", Root));
            status = Status();
            if (status == null) Fail("The daemon did not return an initialized folder.");
        }
        Root = (string)status["root"];
        string remote = (string)status["remote"];
        Step("CH-W09", "Read peer settings", "Check the permissions and backups of your Coven Host local-peer settings, then select Retry.");
        Ordinary(State); Ordinary(PortFile);
        if (remote == null) CreatePeerSettings();
        else if (!Directory.Exists(State)) PrivateDirectory(State);
        bool local = File.Exists(PortFile) && (remote == null || remote == LocalUrl(Port()));
        if (local)
        {
            int port = Port();
            EnsurePeer(port, remote != null);
            if (remote == null)
            {
                Step("CH-W06", "Connect local peer", "Check that the local token is owned by your account and the daemon is available, then select Retry.");
                Success(RunCli("host", "connect", LocalUrl(port), "--token-file", Token));
            }
        }
        if (!resume) Register();
        EnsureWatch();
        Step("CH-W10", "Synchronize folder", "Check your peer or SSH connection. Run coven host status with the installed CLI to inspect pending changes or conflicts, then select Retry.");
        Success(RunCli("host", "sync"));
        Note = local ? "Your folder syncs with a private peer on this PC. Watching resumes when you sign in." : "Your folder syncs with your configured peer. Watching resumes when you sign in.";
    }
}

internal sealed class SetupWindow : Form
{
    readonly Label headline = new Label(), detail = new Label();
    readonly TextBox report = new TextBox();
    readonly FlowLayoutPanel buttons = new FlowLayoutPanel();
    readonly Button folder = new Button(), retry = new Button(), copy = new Button(), save = new Button(), close = new Button();
    readonly bool resume;
    Setup setup;
    bool running;

    internal SetupWindow(bool resumeAtLogin)
    {
        resume = resumeAtLogin;
        Text = "Coven Host";
        Font = new Font("Segoe UI", 10);
        AutoScaleMode = AutoScaleMode.Dpi;
        ClientSize = new Size(620, 390);
        MinimumSize = new Size(560, 400);
        StartPosition = FormStartPosition.CenterScreen;
        var layout = new TableLayoutPanel { Dock = DockStyle.Fill, Padding = new Padding(20), RowCount = 4, ColumnCount = 1 };
        layout.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        layout.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        layout.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
        layout.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        Controls.Add(layout);
        headline.Text = "Opening Coven Host";
        headline.Font = new Font(Font, FontStyle.Bold);
        headline.AutoSize = true;
        detail.AutoSize = true;
        detail.MaximumSize = new Size(550, 0);
        detail.Margin = new Padding(3, 6, 3, 12);
        report.Multiline = true; report.ReadOnly = true; report.Dock = DockStyle.Fill;
        report.ScrollBars = ScrollBars.Vertical; report.AccessibleName = "Diagnostic report";
        buttons.Dock = DockStyle.Fill; buttons.AutoSize = true; buttons.WrapContents = true;
        layout.Controls.Add(headline, 0, 0); layout.Controls.Add(detail, 0, 1);
        layout.Controls.Add(report, 0, 2); layout.Controls.Add(buttons, 0, 3);
        Add(folder, "Open folder", (s, e) => {
            try { Process.Start(new ProcessStartInfo(setup.Root) { UseShellExecute = true }); }
            catch { detail.Text = "Windows could not open the folder. Open it in File Explorer, or select Check again."; }
        });
        Add(copy, "Copy report", (s, e) => {
            try { Clipboard.SetText(report.Text); detail.Text = "Report copied."; }
            catch { report.Focus(); report.SelectAll(); detail.Text = "Select the report and press Ctrl+C, or choose Save report."; }
        });
        Add(save, "Save report", (s, e) => {
            using (var dialog = new SaveFileDialog { FileName = "coven-host-diagnostic.txt", Filter = "Text file (*.txt)|*.txt", DefaultExt = "txt" })
                if (dialog.ShowDialog(this) == DialogResult.OK)
                    try { File.WriteAllText(dialog.FileName, report.Text, Encoding.UTF8); detail.Text = "Report saved."; }
                    catch { detail.Text = "Choose another save location or select and copy the report."; }
        });
        Add(retry, "Retry", (s, e) => BeginSetup(false));
        Add(close, "Close", (s, e) => Close());
        FormClosing += (s, e) => { if (running && e.CloseReason == CloseReason.UserClosing) e.Cancel = true; };
        Shown += (s, e) => { if (resume) Hide(); BeginSetup(resume); };
    }

    void Add(Button button, string text, EventHandler clicked)
    {
        button.Text = text; button.AutoSize = true; button.Click += clicked; buttons.Controls.Add(button);
    }

    async void BeginSetup(bool atLogin)
    {
        running = true;
        buttons.Visible = false; report.Visible = false;
        headline.Text = "Setting up Coven Host";
        detail.Text = "Checking this installation.";
        setup = new Setup(stage => BeginInvoke(new Action(() => detail.Text = stage)));
        bool ok = await Task.Run(() => setup.Run(atLogin));
        running = false;
        if (atLogin && ok) { Close(); return; }
        if (!Visible) Show();
        buttons.Visible = true;
        folder.Visible = ok; copy.Visible = save.Visible = report.Visible = !ok;
        headline.Text = ok ? "Your Coven folder is ready" : "Coven Host needs attention";
        detail.Text = ok ? setup.Note + " Edit documents in Castles\\demo\\files or Castles\\demo\\results." : "Copy or save this report, then follow its next step.";
        retry.Text = ok ? "Check again" : "Retry";
        report.Text = ok ? "" : setup.Report;
        if (ok) { AcceptButton = folder; folder.Focus(); }
        else { AcceptButton = retry; report.Select(0, 0); }
    }

    [STAThread]
    static void Main(string[] args)
    {
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        Application.Run(new SetupWindow(args.Length == 1 && args[0] == "--resume"));
    }
}
