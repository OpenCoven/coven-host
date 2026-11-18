import Cocoa
import CryptoKit
import Security
import Darwin

private let fm = FileManager.default
#if arch(arm64)
let architecture = "arm64"
#else
let architecture = "x86_64"
#endif
private let qualifiedArm = "e34b85823a362e6fef541adb70d6e6787f5eb9515dbc16ae859fb24d19806b18"

enum Stage: String {
    case profile, application, payload, credentials, peer, folder, connection, watcher, verification, finder

    var expected: String {
        switch self {
        case .profile: return "An available, private local installation profile"
        case .application: return "A verified copy of Coven Host in your Applications folder"
        case .payload: return "The verified native Coven executable for this Mac"
        case .credentials: return "An owner-only credential for the local peer"
        case .peer: return "A running login service with its original peer storage"
        case .folder: return "An ordinary Coven folder bound to this profile"
        case .connection: return "An authenticated connection to the configured local peer"
        case .watcher: return "A running folder watcher registered for login"
        case .verification: return "A completed reconciliation with the local peer"
        case .finder: return "The synchronized folder open in Finder"
        }
    }
}

struct SetupFailure: Error {
    let stage: Stage
    let code: String
    let observed: String
    let action: String
    let classification: String
    let date = Date()

    init(_ stage: Stage, _ code: String, _ observed: String, _ action: String, _ classification: String) {
        self.stage = stage
        self.code = "CH-MAC-" + code
        self.observed = observed
        self.action = action
        self.classification = classification
    }

    var report: String {
        let os = ProcessInfo.processInfo.operatingSystemVersion
        return [
            "Coven Host installer 0.1.1", "Platform: macOS \(os.majorVersion).\(os.minorVersion).\(os.patchVersion), \(architecture)",
            "UTC time: " + ISO8601DateFormatter().string(from: date), "Stage: " + stage.rawValue,
            "Code: " + code, "Expected: " + stage.expected, "Observed: " + observed,
            "Error classification: " + classification, "Reproduction: Open the Coven Host installer while signed in to this Mac.",
            "Next action: " + action
        ].joined(separator: "\n")
    }
}

private func osFailure(_ stage: Stage, _ error: Error) -> SetupFailure {
    var e = error as NSError
    if let underlying = e.userInfo[NSUnderlyingErrorKey] as? NSError { e = underlying }
    let kind: String
    switch e.domain {
    case NSPOSIXErrorDomain: kind = "POSIX \(e.code)"
    case NSCocoaErrorDomain: kind = "Cocoa \(e.code)"
    default: kind = "System error \(e.code)"
    }
    return SetupFailure(stage, "IO", "macOS could not finish this operation.", "Check free disk space and access to your account folders, then reopen the installer.", kind)
}

private func digest(_ data: Data) -> String {
    SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
}

private func info(_ file: URL) throws -> stat? {
    var s = stat()
    if lstat(file.path, &s) == 0 { return s }
    if errno == ENOENT { return nil }
    throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno))
}

private func requireType(_ file: URL, _ type: mode_t, privateOnly: Bool = false) throws {
    guard let s = try info(file), s.st_mode & S_IFMT == type, s.st_uid == getuid(),
          !privateOnly || s.st_mode & 0o077 == 0,
          type != S_IFREG || s.st_nlink == 1 else {
        throw SetupFailure(.profile, "UNSAFE-ITEM", "An installation item has an unexpected type, owner or access mode.", "Keep your existing files and ask support to review the installation permissions.", "Filesystem ownership or type")
    }
}

// Never follow a link in the per-user installation hierarchy or alter existing directory permissions.
private func directories(_ dir: URL, under home: URL, privateOnly: Bool = false) throws {
    if dir.path == home.path { return }
    guard dir.path.hasPrefix(home.path + "/") else {
        throw SetupFailure(.profile, "LOCATION", "The installation location is outside the user profile.", "Open the installer from your normal user account.", "Invalid location")
    }
    try directories(dir.deletingLastPathComponent(), under: home)
    if try info(dir) == nil {
        if mkdir(dir.path, 0o700) != 0 && errno != EEXIST { throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno)) }
    }
    try requireType(dir, S_IFDIR, privateOnly: privateOnly)
}

// A fully written file becomes visible only after the exclusive same-directory rename.
private func publish(_ data: Data, at path: URL, mode: mode_t = 0o600) throws {
    let temp = path.deletingLastPathComponent().appendingPathComponent(".setup-" + UUID().uuidString)
    let fd = open(temp.path, O_CREAT | O_EXCL | O_NOFOLLOW | O_WRONLY, mode)
    guard fd >= 0 else { throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno)) }
    let stream = FileHandle(fileDescriptor: fd, closeOnDealloc: true)
    defer { try? fm.removeItem(at: temp) }
    try stream.write(contentsOf: data)
    try stream.synchronize()
    try stream.close()
    guard renamex_np(temp.path, path.path, UInt32(RENAME_EXCL)) == 0 else {
        throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno))
    }
}

private struct Reply {
    let status: Int32
    let bytes: Data
    var text: String { String(decoding: bytes, as: UTF8.self) }
}

private func run(_ exe: String, _ args: [String], env: [String: String], timeout: Double = 45) throws -> Reply {
    let proc = Process()
    proc.executableURL = URL(fileURLWithPath: exe)
    proc.arguments = args
    proc.environment = env
    proc.standardInput = FileHandle.nullDevice
    // An unlinked private descriptor avoids pipe deadlocks and persistence of native status or file names.
    let tmp = fm.temporaryDirectory.appendingPathComponent("coven-setup-" + UUID().uuidString)
    let fd = open(tmp.path, O_CREAT | O_EXCL | O_RDWR | O_NOFOLLOW, 0o600)
    guard fd >= 0 else { throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno)) }
    let out = FileHandle(fileDescriptor: fd, closeOnDealloc: true)
    guard unlink(tmp.path) == 0 else { throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno)) }
    proc.standardOutput = out
    proc.standardError = out
    try proc.run()
    let end = ProcessInfo.processInfo.systemUptime + timeout
    while proc.isRunning && ProcessInfo.processInfo.systemUptime < end { Thread.sleep(forTimeInterval: 0.05) }
    if proc.isRunning {
        proc.terminate()
        Thread.sleep(forTimeInterval: 0.1)
        if proc.isRunning { kill(proc.processIdentifier, SIGKILL) }
        proc.waitUntilExit()
        return Reply(status: 124, bytes: Data())
    }
    proc.waitUntilExit()
    try out.seek(toOffset: 0)
    return Reply(status: proc.terminationStatus, bytes: try out.read(upToCount: 16 * 1024 * 1024) ?? Data())
}

private func portAvailable() throws -> Int {
    let fd = socket(AF_INET, SOCK_STREAM, 0)
    guard fd >= 0 else { throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno)) }
    defer { close(fd) }
    var address = sockaddr_in()
    address.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
    address.sin_family = sa_family_t(AF_INET)
    address.sin_addr.s_addr = inet_addr("127.0.0.1")
    var length = socklen_t(MemoryLayout<sockaddr_in>.size)
    let result = withUnsafeMutablePointer(to: &address) { pointer in
        pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
            Darwin.bind(fd, $0, length) == 0 && getsockname(fd, $0, &length) == 0
        }
    }
    guard result else { throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno)) }
    return Int(UInt16(bigEndian: address.sin_port))
}

private func classify(_ reply: Reply) -> String {
    if reply.status == 124 { return "Command timeout" }
    let message = reply.text.lowercased()
    for (needle, kind) in [("permission denied", "Filesystem permission"), ("operation not permitted", "macOS permission"), ("unauthorized", "Peer authentication"), ("401", "Peer authentication"), ("refused", "Peer unavailable"), ("conflict", "Synchronization conflict"), ("identity", "Peer identity"), ("rolled back", "Peer history"), ("no space", "Disk space"), ("address already in use", "Local port in use"), ("socket", "Local socket"), ("not initialized", "Uninitialized profile")] {
        if message.contains(needle) { return kind + " (exit \(reply.status))" }
    }
    return "Command exit \(reply.status)"
}

final class Installer {
    let home: URL
    let prefix: String
    let bundle: Bundle
    var stage = Stage.profile
    let progress: (Stage) -> Void

    init(home: URL, prefix: String, bundle: Bundle, progress: @escaping (Stage) -> Void) {
        self.home = home
        self.prefix = prefix
        self.bundle = bundle
        self.progress = progress
    }

    private func step(_ value: Stage) { stage = value; progress(value) }
    private var root: URL { home.appendingPathComponent("Coven") }
    private var state: URL { home.appendingPathComponent(".local/state/coven-host-mvp") }
    private var support: URL { home.appendingPathComponent("Library/Application Support/Coven Host") }
    private var token: URL { state.appendingPathComponent("access/token") }
    private var server: URL { state.appendingPathComponent("server") }
    private var agents: URL { home.appendingPathComponent("Library/LaunchAgents") }
    private var env: [String: String] { ["PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "HOME": home.path, "COVEN_HOME": state.appendingPathComponent("profile").path, "COVEN_HOST_STATE": state.appendingPathComponent("client").path] }
    private var domain: String { "gui/\(getuid())" }
    private func plist(_ role: String) -> URL { agents.appendingPathComponent(prefix + "." + role + ".plist") }

    private func checked(_ exe: URL, _ args: [String]) throws -> Reply {
        let r = try run(exe.path, args, env: env)
        guard r.status == 0 else {
            throw SetupFailure(stage, "COMMAND-" + stage.rawValue.uppercased(), "The native service could not complete this operation.", "Reopen the installer. If the message returns, share the diagnostics with support.", classify(r))
        }
        return r
    }

    private func preserve(_ note: String) -> SetupFailure {
        SetupFailure(stage, "EXISTING-SETUP", note, "Keep the existing folder and service files. Ask support to review the earlier setup before installing again.", "Existing configuration mismatch")
    }

    private func readJob(_ role: String) throws -> [String: Any]? {
        let p = plist(role)
        if try info(p) == nil { return nil }
        try requireType(p, S_IFREG)
        guard let data = try PropertyListSerialization.propertyList(from: Data(contentsOf: p), format: nil) as? [String: Any] else { throw preserve("A saved login service could not be recognized.") }
        return data
    }

    private func job(_ role: String, _ binary: URL, _ port: Int) -> [String: Any] {
        let args = role == "peer" ? ["serve", "--port", String(port), "--storage", server.path, "--token-file", token.path] : ["watch"]
        return ["Label": prefix + "." + role, "ProgramArguments": [binary.path, "host"] + args,
                "EnvironmentVariables": ["COVEN_HOME": env["COVEN_HOME"]!, "COVEN_HOST_STATE": env["COVEN_HOST_STATE"]!],
                "RunAtLoad": true, "KeepAlive": true, "ThrottleInterval": 5]
    }

    @discardableResult
    private func activate(_ role: String, _ config: [String: Any]) throws -> Int {
        let path = plist(role)
        let target = domain + "/" + prefix + "." + role
        var observation = try run("/bin/launchctl", ["print", target], env: env)
        if try info(path) == nil {
            guard observation.status != 0 else { throw preserve("The service label already belongs to another login service.") }
            try publish(PropertyListSerialization.data(fromPropertyList: config, format: .xml, options: 0), at: path)
        }
        if observation.status != 0 {
            _ = try checked(URL(fileURLWithPath: "/bin/launchctl"), ["bootstrap", domain, path.path])
        }
        let end = ProcessInfo.processInfo.systemUptime + 12
        repeat {
            observation = try run("/bin/launchctl", ["print", target], env: env)
            let lines = observation.text.components(separatedBy: .newlines).map { $0.trimmingCharacters(in: .whitespaces) }
            if observation.status == 0 {
                guard lines.contains("path = " + path.path) else { throw preserve("A different login service uses this label.") }
                if lines.contains("state = running"), let value = lines.first(where: { $0.hasPrefix("pid = ") }), let pid = Int(value.dropFirst(6)) { return pid }
            }
            Thread.sleep(forTimeInterval: 0.2)
        } while ProcessInfo.processInfo.systemUptime < end
        throw SetupFailure(stage, "SERVICE-START", "The login service did not stay running.", "Check that Coven Host is allowed in Login Items, then reopen the installer.", "launchd service startup")
    }

    private func validApp(_ url: URL) throws -> Bool {
        try run("/usr/bin/codesign", ["--verify", "--all-architectures", "--deep", "--strict", url.path], env: env).status == 0
    }

    private func installApp() throws {
        let source = bundle.bundleURL
        let applications = home.appendingPathComponent("Applications")
        try directories(applications, under: home)
        let executable = "Contents/MacOS/CovenHostSetup"
        // The universal executable seals the resource manifest and Info.plist in each slice's code signature.
        guard try validApp(source) else {
            throw SetupFailure(stage, "APP-SIGNATURE", "The application signature could not be verified.", "Download a fresh copy of Coven Host from the private release.", "Invalid bundle signature")
        }
        let destinationNames = ["Coven Host.app", "Coven Host Setup.app"]
        let staging = applications.appendingPathComponent(".coven-host-" + UUID().uuidString)
        var staged = false
        defer { if staged { try? fm.removeItem(at: staging) } }
        let copy = staging.appendingPathComponent("Coven Host.app")
        for name in destinationNames {
            let destination = applications.appendingPathComponent(name)
            if try info(destination) == nil {
                if !staged {
                    try fm.createDirectory(at: staging, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
                    staged = true
                    _ = try checked(URL(fileURLWithPath: "/usr/bin/ditto"), ["--rsrc", "--extattr", source.path, copy.path])
                    guard try validApp(copy) else {
                        throw SetupFailure(stage, "APP-COPY", "The copied application did not pass signature verification.", "Check free disk space and download a fresh copy of Coven Host.", "Copied bundle signature")
                    }
                }
                if renamex_np(copy.path, destination.path, UInt32(RENAME_EXCL)) == 0 { return }
                if errno != EEXIST { throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno)) }
            }
            if let item = try info(destination), item.st_mode & S_IFMT == S_IFDIR, item.st_uid == getuid(),
               fm.contentsEqual(atPath: source.appendingPathComponent(executable).path, andPath: destination.appendingPathComponent(executable).path),
               try validApp(destination) { return }
        }
        throw SetupFailure(stage, "APP-NAME", "Both application names are occupied in your Applications folder.", "Move or rename one of the existing applications yourself, then reopen the downloaded installer.", "Application name collision")
    }

    func install() throws -> URL {
        do { return try installChecked() }
        catch let f as SetupFailure { throw f }
        catch { throw osFailure(stage, error) }
    }

    private func installChecked() throws -> URL {
        step(.profile)
        try directories(support, under: home)
        let lock = open(support.path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW)
        guard lock >= 0 else { throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno)) }
        defer { close(lock) }
        guard flock(lock, LOCK_EX | LOCK_NB) == 0 else {
            throw SetupFailure(stage, "BUSY", "Another setup is in progress.", "Wait for the open installer to finish, then reopen it.", "Installer lock")
        }
        step(.application)
        try installApp()
        step(.profile)
        try directories(agents, under: home)
        let previousPeer = try readJob("peer")
        let previousWatch = try readJob("watch")
        let marker = state.appendingPathComponent("installer-port")
        let oldBinary = support.appendingPathComponent("releases/v0.1.0/node_modules/@opencoven/cli-macos/bin/coven")
        let newBinary = support.appendingPathComponent("releases/v0.1.1/bin/coven")
        let existingState = try info(state) != nil
        if existingState { try directories(state, under: home, privateOnly: true) }
        let managed = try info(marker) != nil
        let legacy = !managed && previousPeer != nil && previousWatch != nil
        let binary = legacy ? oldBinary : newBinary
        let port: Int
        if managed {
            try requireType(marker, S_IFREG, privateOnly: true)
            guard let number = Int(try String(contentsOf: marker, encoding: .utf8)), (1...65535).contains(number) else { throw preserve("The saved local port is invalid.") }
            port = number
        } else if legacy {
            guard let args = previousPeer?["ProgramArguments"] as? [String], args.count == 9, let number = Int(args[4]), (1...65535).contains(number) else { throw preserve("The earlier peer settings could not be recognized.") }
            port = number
        } else {
            guard !existingState, previousPeer == nil, previousWatch == nil else { throw preserve("An incomplete earlier install needs review.") }
            // The v0.1.0 manual guide used a different daemon profile. Do not attach another client to its folder.
            guard try info(home.appendingPathComponent(".local/state/coven-host/client/client.sqlite")) == nil,
                  try info(home.appendingPathComponent(".local/state/coven-host/profile")) == nil else { throw preserve("A manually configured Coven Host profile is present.") }
            if try info(root) != nil {
                try requireType(root, S_IFDIR)
                guard try fm.contentsOfDirectory(atPath: root.path).isEmpty else { throw preserve("A Coven folder already contains files and has no recognized installer profile.") }
            }
            port = try portAvailable()
            try directories(state, under: home, privateOnly: true)
            try publish(Data(String(port).utf8), at: marker)
        }
        let peer = job("peer", binary, port)
        let watch = job("watch", binary, port)
        for (existing, wanted) in [(previousPeer, peer), (previousWatch, watch)] {
            if let existing, !NSDictionary(dictionary: existing).isEqual(to: wanted) { throw preserve("The saved login service uses different settings.") }
        }
        step(.payload)
        guard let qualifiedSHA = bundle.object(forInfoDictionaryKey: "CovenPayloadSHA-" + architecture) as? String else {
            throw SetupFailure(stage, "MISSING-PAYLOAD", "This installer lacks a native executable for this Mac.", "Download the installer for your Mac from the private release.", "Missing architecture metadata")
        }
        let resource = bundle.bundleURL.appendingPathComponent("Contents/Helpers/coven-" + architecture)
        let payload = try Data(contentsOf: resource, options: .mappedIfSafe)
        guard digest(payload) == qualifiedSHA else {
            throw SetupFailure(stage, "PAYLOAD", "The bundled executable failed its integrity check.", "Download a fresh copy of the installer from the private release.", "SHA-256 mismatch")
        }
        if try info(binary) == nil {
            guard !legacy else { throw preserve("The earlier installation is missing its executable.") }
            try directories(binary.deletingLastPathComponent(), under: home)
            try publish(payload, at: binary, mode: 0o700)
        }
        try requireType(binary, S_IFREG)
        let installedHash = digest(try Data(contentsOf: binary, options: .mappedIfSafe))
        guard installedHash == qualifiedSHA || (legacy && architecture == "arm64" && installedHash == qualifiedArm) else { throw preserve("The installed executable has unexpected contents.") }
        step(.credentials)
        for child in ["client", "profile", "server"] {
            let directory = state.appendingPathComponent(child)
            if try info(directory) != nil { try directories(directory, under: home, privateOnly: true) }
        }
        let clientDB = state.appendingPathComponent("client/client.sqlite")
        let peerDB = server.appendingPathComponent("host.sqlite")
        if legacy {
            try requireType(clientDB, S_IFREG)
            try requireType(peerDB, S_IFREG)
        }
        try directories(state.appendingPathComponent("access"), under: home, privateOnly: true)
        if try info(token) == nil {
            guard !legacy, try info(clientDB) == nil, try info(peerDB) == nil, previousPeer == nil else { throw preserve("The existing peer credential is missing.") }
            var random = [UInt8](repeating: 0, count: 32)
            guard SecRandomCopyBytes(kSecRandomDefault, random.count, &random) == errSecSuccess else {
                throw SetupFailure(stage, "RANDOM", "The system could not generate a credential.", "Reopen the installer.", "Security framework random source")
            }
            try publish(Data((random.map { String(format: "%02x", $0) }.joined() + "\n").utf8), at: token)
        }
        try requireType(token, S_IFREG, privateOnly: true)
        let hasClientDB = try info(clientDB) != nil
        if previousWatch != nil || hasClientDB {
            guard try info(peerDB) != nil else { throw preserve("The persistent storage for this installation is missing.") }
        }
        if previousWatch != nil {
            guard try info(clientDB) != nil, try info(root) != nil else { throw preserve("The configured folder or client database is missing.") }
        }
        if try info(root) != nil { try requireType(root, S_IFDIR) }
        step(.peer)
        let peerPID = try activate("peer", peer)
        try waitForListener(peerPID, port)
        let endpoint = "http://127.0.0.1:\(port)"
        // Authenticate the listener before configuring a client. Only the status code is retained.
        let secretData = try Data(contentsOf: token)
        guard secretData.count <= 130, let secret = String(data: secretData, encoding: .utf8)?.trimmingCharacters(in: .newlines),
              (43...128).contains(secret.utf8.count), secret.range(of: "^[A-Za-z0-9_-]+$", options: .regularExpression) != nil else {
            throw SetupFailure(.credentials, "TOKEN-FORMAT", "The existing credential has an unexpected format.", "Restore the matching protected credential from your backup.", "Credential format")
        }
        try authenticate(endpoint, secret)
        step(.folder)
        if try info(clientDB) == nil {
            _ = try checked(binary, ["host", "init", "--root", root.path])
        }
        let status = try checked(binary, ["host", "status"])
        guard let details = try JSONSerialization.jsonObject(with: status.bytes) as? [String: Any], details["root"] as? String == root.path else { throw preserve("The client is bound to another folder.") }
        step(.connection)
        if let configured = details["remote"] as? String {
            guard configured == endpoint else { throw preserve("The existing client uses a different peer.") }
        } else {
            guard !legacy, previousWatch == nil else { throw preserve("The earlier client connection is missing.") }
            _ = try checked(binary, ["host", "connect", endpoint, "--token-file", token.path])
        }
        step(.verification)
        // A healthy existing watcher already reconciles the folder. Keep that session in place.
        if previousWatch == nil || details["phase"] as? String != "synced" {
            _ = try checked(binary, ["host", "sync"])
        }
        step(.watcher)
        try activate("watch", watch)
        return root
    }

    // Check the launchd process owns the loopback listener before sharing a credential. A port collision may have started an unrelated service.
    private func waitForListener(_ pid: Int, _ port: Int) throws {
        let until = ProcessInfo.processInfo.systemUptime + 8
        repeat {
            let r = try run("/usr/sbin/lsof", ["-nP", "-a", "-p", String(pid), "-iTCP:" + String(port), "-sTCP:LISTEN", "-Fn"], env: env, timeout: 3)
            if r.status == 0 && r.text.components(separatedBy: .newlines).contains("n127.0.0.1:" + String(port)) { return }
            Thread.sleep(forTimeInterval: 0.2)
        } while ProcessInfo.processInfo.systemUptime < until
        throw SetupFailure(stage, "LISTENER", "The registered peer does not own the expected loopback listener.", "Close any other service using the local peer port, then reopen the installer.", "Peer listener ownership")
    }

    private func authenticate(_ endpoint: String, _ secret: String) throws {
        let cfg = URLSessionConfiguration.ephemeral
        cfg.connectionProxyDictionary = ["HTTPEnable": 0, "HTTPSEnable": 0, "SOCKSEnable": 0]
        cfg.timeoutIntervalForRequest = 1
        let session = URLSession(configuration: cfg, delegate: LocalOnlyDelegate(), delegateQueue: nil)
        defer { session.invalidateAndCancel() }
        let until = ProcessInfo.processInfo.systemUptime + 10
        var http = 0
        repeat {
            let done = DispatchSemaphore(value: 0)
            var req = URLRequest(url: URL(string: endpoint + "/v1/castles/demo/files")!)
            req.setValue("Bearer " + secret, forHTTPHeaderField: "Authorization")
            let result = HTTPResult()
            session.dataTask(with: req) { _, response, _ in result.status = (response as? HTTPURLResponse)?.statusCode ?? 0; done.signal() }.resume()
            done.wait()
            http = result.status
            if http == 200 { return }
            if http != 0 { break }
            Thread.sleep(forTimeInterval: 0.15)
        } while ProcessInfo.processInfo.systemUptime < until
        throw SetupFailure(stage, "PEER-READY", "The local peer did not accept the authenticated check.", "Reopen the installer. If the issue repeats, check the local service and its original credential.", http == 0 ? "Loopback connection unavailable" : "Local HTTP \(http)")
    }
}

private final class HTTPResult: @unchecked Sendable { var status = 0 }
private final class LocalOnlyDelegate: NSObject, URLSessionTaskDelegate {
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) { completionHandler(nil) }
}

enum Diagnostics {
    static func copy(_ report: String, to pasteboard: NSPasteboard) -> Bool {
        pasteboard.clearContents()
        return pasteboard.setString(report, forType: .string)
    }

    static func save(_ report: String, to file: URL) throws {
        try report.write(to: file, atomically: true, encoding: .utf8)
    }
}
