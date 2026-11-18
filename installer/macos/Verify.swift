import Cocoa
import CryptoKit

// The CLI verifier links the installation engine and exercises the exact downloaded app's signed payload.
// It always uses disposable paths and launchd labels. It never presents or activates windows.
@main
struct Verify {
    static func main() throws {
        umask(0o077)
        guard CommandLine.arguments.count == 3, let bundle = Bundle(path: CommandLine.arguments[1]) else { exit(2) }
        let path = CommandLine.arguments[2]
        var s = stat()
        guard path.range(of: "^/private/tmp/ch-mac\\.[A-Za-z0-9_-]+$", options: .regularExpression) != nil,
              lstat(path, &s) == 0, s.st_mode & S_IFMT == S_IFDIR, s.st_uid == getuid(), s.st_mode & 0o077 == 0 else { exit(2) }
        let id = SHA256.hash(data: Data(path.utf8)).prefix(6).map { String(format: "%02x", $0) }.joined()
        let home = URL(fileURLWithPath: path)
        let installer = Installer(home: home, prefix: "org.opencoven.coven-host.test." + id, bundle: bundle) { _ in }
        do {
            _ = try installer.install()
            print("Installed and verified")
        } catch let error as SetupFailure {
            let report = error.report
            let board = NSPasteboard.withUniqueName()
            defer { board.releaseGlobally() }
            guard Diagnostics.copy(report, to: board), board.string(forType: .string) == report else { exit(3) }
            let file = home.appendingPathComponent("diagnostics.txt")
            try Diagnostics.save(report, to: file)
            guard try String(contentsOf: file, encoding: .utf8) == report else { exit(4) }
            print(report)
            exit(1)
        }
    }
}
