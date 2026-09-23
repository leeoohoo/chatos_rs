import AppKit
import Darwin
import Foundation
import SwiftTerm
import SwiftUI

@MainActor
final class NativeLocalTerminalViewModel: ObservableObject {
    enum State: Equatable {
        case idle
        case running
        case exited(Int32?)
        case failed(String)
        case closed
    }

    @Published private(set) var state: State = .idle
    @Published private(set) var title: String
    @Published private(set) var workingDirectory: String
    @Published var searchText = "" {
        didSet { updateSearch() }
    }

    private let initialWorkingDirectory: String
    private var observer: NativeTerminalProcessObserver?
    private var hasStarted = false

    private(set) lazy var terminalView: LocalProcessTerminalView = {
        var options = TerminalOptions.default
        options.scrollback = 10_000
        let view = RestrictedLocalProcessTerminalView(
            frame: .zero,
            font: NSFont.monospacedSystemFont(ofSize: 13, weight: .regular),
            options: options
        )
        view.autoresizingMask = [.width, .height]
        view.nativeForegroundColor = .textColor
        view.nativeBackgroundColor = .textBackgroundColor
        view.optionAsMetaKey = true
        view.allowMouseReporting = true
        view.linkReporting = .implicit
        let observer = NativeTerminalProcessObserver(owner: self)
        self.observer = observer
        view.processDelegate = observer
        return view
    }()

    init(workingDirectory: String) {
        let normalized = URL(fileURLWithPath: workingDirectory).standardizedFileURL.path
        self.initialWorkingDirectory = normalized
        self.workingDirectory = normalized
        let directoryName = URL(fileURLWithPath: normalized).lastPathComponent
        self.title = directoryName.isEmpty ? "终端" : directoryName
    }

    var statusTitle: String {
        switch state {
        case .idle: "准备中"
        case .running: "已连接"
        case .exited: "已退出"
        case .failed: "启动失败"
        case .closed: "已关闭"
        }
    }

    var statusColor: SwiftUI.Color {
        switch state {
        case .running: .green
        case .idle: .secondary
        case .exited, .closed: .orange
        case .failed: .red
        }
    }

    var shellName: String {
        URL(fileURLWithPath: resolvedShellPath()).lastPathComponent
    }

    func startIfNeeded() {
        guard !hasStarted, state == .idle else { return }
        hasStarted = true
        let shell = resolvedShellPath()
        let view = terminalView
        view.startProcess(
            executable: shell,
            args: ["-l"],
            environment: terminalEnvironment(),
            currentDirectory: initialWorkingDirectory
        )
        if view.process?.running == true {
            state = .running
        } else {
            state = .failed("无法启动登录 Shell：\(shell)")
        }
    }

    func focus() {
        terminalView.window?.makeFirstResponder(terminalView)
    }

    func interrupt() {
        guard state == .running else { return }
        let interrupt: [UInt8] = [3]
        terminalView.process?.send(data: interrupt[...])
    }

    func clear() {
        terminalView.terminal.clearScrollback()
        terminalView.feed(text: "\u{1B}[2J\u{1B}[H")
    }

    func findNext() {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else { return }
        terminalView.findNext(query)
    }

    func findPrevious() {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else { return }
        terminalView.findPrevious(query)
    }

    func close() {
        guard state != .closed else { return }
        if let process = terminalView.process,
           process.running,
           process.shellPid > 0 {
            // SwiftTerm creates a new session for the PTY child. Signalling the
            // process group prevents jobs started by the shell from surviving
            // an explicit terminal-tab close.
            _ = Darwin.kill(-process.shellPid, SIGHUP)
        }
        terminalView.terminate()
        state = .closed
    }

    fileprivate func processTerminated(exitCode: Int32?) {
        guard state != .closed else { return }
        state = .exited(exitCode)
    }

    fileprivate func updateTitle(_ value: String) {
        let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines)
        if !normalized.isEmpty { title = normalized }
    }

    fileprivate func updateWorkingDirectory(_ value: String?) {
        guard let value else { return }
        let path: String
        if let url = URL(string: value), url.isFileURL {
            path = url.path
        } else {
            path = value
        }
        let normalized = path.trimmingCharacters(in: .whitespacesAndNewlines)
        if !normalized.isEmpty { workingDirectory = normalized }
    }

    private func updateSearch() {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        if query.isEmpty {
            terminalView.clearSearch()
        } else {
            terminalView.findNext(query)
        }
    }

    private func resolvedShellPath() -> String {
        let configured = ProcessInfo.processInfo.environment["SHELL"] ?? ""
        if configured.hasPrefix("/"), FileManager.default.isExecutableFile(atPath: configured) {
            return configured
        }
        return "/bin/zsh"
    }

    private func terminalEnvironment() -> [String] {
        var environment = ProcessInfo.processInfo.environment
        environment["TERM"] = "xterm-256color"
        environment["COLORTERM"] = "truecolor"
        return environment
            .map { "\($0.key)=\($0.value)" }
            .sorted()
    }
}

struct NativeLocalTerminalSurface: NSViewRepresentable {
    @ObservedObject var terminal: NativeLocalTerminalViewModel

    func makeNSView(context: Context) -> LocalProcessTerminalView {
        let view = terminal.terminalView
        terminal.startIfNeeded()
        DispatchQueue.main.async { terminal.focus() }
        return view
    }

    func updateNSView(_ view: LocalProcessTerminalView, context: Context) {
        view.nativeForegroundColor = .textColor
        view.nativeBackgroundColor = .textBackgroundColor
    }

    static func dismantleNSView(_ view: LocalProcessTerminalView, coordinator: Void) {
        // Switching resources only detaches the AppKit view. The owning tab
        // explicitly closes the PTY through NativeLocalTerminalViewModel.
    }
}

private final class NativeTerminalProcessObserver: NSObject, LocalProcessTerminalViewDelegate {
    private weak var owner: NativeLocalTerminalViewModel?

    init(owner: NativeLocalTerminalViewModel) {
        self.owner = owner
    }

    func sizeChanged(source: LocalProcessTerminalView, newCols: Int, newRows: Int) {}

    func setTerminalTitle(source: LocalProcessTerminalView, title: String) {
        Task { @MainActor [weak owner] in owner?.updateTitle(title) }
    }

    func hostCurrentDirectoryUpdate(source: TerminalView, directory: String?) {
        Task { @MainActor [weak owner] in owner?.updateWorkingDirectory(directory) }
    }

    func processTerminated(source: TerminalView, exitCode: Int32?) {
        Task { @MainActor [weak owner] in owner?.processTerminated(exitCode: exitCode) }
    }
}

private final class RestrictedLocalProcessTerminalView: LocalProcessTerminalView {
    override func requestOpenLink(source: TerminalView, link: String, params: [String: String]) {
        guard let url = URL(string: link),
              let scheme = url.scheme?.lowercased(),
              ["http", "https", "mailto"].contains(scheme) else {
            return
        }
        NSWorkspace.shared.open(url)
    }
}
