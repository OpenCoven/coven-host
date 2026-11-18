import Cocoa
import CryptoKit

final class App: NSObject, NSApplicationDelegate {
    var window: NSWindow!
    let label = NSTextField(labelWithString: "Preparing your Coven folder…")

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
        let activity = NSProgressIndicator()
        activity.style = .spinning
        activity.controlSize = .regular
        activity.startAnimation(nil)
        let stack = NSStackView(views: [activity, label])
        stack.spacing = 15
        stack.edgeInsets = NSEdgeInsets(top: 24, left: 28, bottom: 24, right: 28)
        label.font = .systemFont(ofSize: 14)
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 380, height: 80), styleMask: [.titled], backing: .buffered, defer: false)
        window.title = "Coven Host"
        window.contentView = stack
        window.center()
        window.makeKeyAndOrderFront(nil)
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                let root = try Installer(home: FileManager.default.homeDirectoryForCurrentUser, prefix: "org.opencoven.coven-host", bundle: .main) { stage in
                    DispatchQueue.main.async { self.label.stringValue = self.title(stage) }
                }.install()
                DispatchQueue.main.async {
                    if NSWorkspace.shared.open(root) { NSApp.terminate(nil) }
                    else { self.failed(SetupFailure(.finder, "FINDER", "Your folder is ready, but Finder could not open it.", "Open the Coven folder in your home directory.", "Workspace open failure")) }
                }
            } catch {
                let failure = error as? SetupFailure ?? SetupFailure(.profile, "SETUP", "Setup could not finish.", "Reopen the installer and share the diagnostics if this repeats.", "Unexpected setup error")
                DispatchQueue.main.async { self.failed(failure) }
            }
        }
    }

    private func title(_ stage: Stage) -> String {
        switch stage {
        case .profile: return "Checking your installation…"
        case .application: return "Installing Coven Host in Applications…"
        case .payload: return "Verifying Coven Host…"
        case .credentials: return "Checking private access…"
        case .peer: return "Starting the local peer…"
        case .folder: return "Preparing your Coven folder…"
        case .connection: return "Connecting your folder…"
        case .watcher: return "Starting automatic sync…"
        case .verification: return "Checking synchronization…"
        case .finder: return "Opening your folder…"
        }
    }

    private func failed(_ f: SetupFailure) {
        window.orderOut(nil)
        NSApp.activate(ignoringOtherApps: true)
        let report = f.report
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = "Coven Host needs attention"
        alert.informativeText = f.observed + " " + f.action
        let text = NSTextView(frame: NSRect(x: 0, y: 0, width: 410, height: 196))
        text.font = .systemFont(ofSize: 11)
        text.string = report
        text.isEditable = false
        text.isSelectable = true
        text.autoresizingMask = [.width]
        let scroll = NSScrollView(frame: text.frame)
        scroll.hasVerticalScroller = true
        scroll.documentView = text
        alert.accessoryView = scroll
        alert.addButton(withTitle: "Copy Diagnostics")
        alert.addButton(withTitle: "Save Diagnostics…")
        alert.addButton(withTitle: "Close")
        while true {
            switch alert.runModal() {
            case .alertFirstButtonReturn:
                if Diagnostics.copy(report, to: .general) {
                    NSApp.terminate(nil)
                    return
                }
                alert.informativeText = "Copying failed. Select and copy the text below or save a diagnostics file."
            case .alertSecondButtonReturn:
                let panel = NSSavePanel()
                panel.nameFieldStringValue = "Coven-Host-diagnostics.txt"
                if panel.runModal() == .OK, let url = panel.url {
                    do { try Diagnostics.save(report, to: url); NSApp.terminate(nil); return }
                    catch { alert.informativeText = "The file could not be saved. Choose another location or copy the text below." }
                }
            default: NSApp.terminate(nil); return
            }
        }
    }
}

umask(0o077)
let app = NSApplication.shared
let delegate = App()
app.delegate = delegate
app.run()
