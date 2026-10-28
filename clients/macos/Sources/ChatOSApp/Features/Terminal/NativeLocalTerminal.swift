import AppKit
import ChatOSCore
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
    @Published private(set) var shellName = "Shell"
    @Published var searchText = "" {
        didSet { updateSearch() }
    }

    let id: UUID

    private let initialWorkingDirectory: String
    private let useCase: LocalTerminalUseCase
    private var lifecycle = LocalTerminalLifecycleState()
    private var session: (any LocalTerminalSessionPort)?
    private var eventTask: Task<Void, Never>?
    private var hasStarted = false

    private(set) lazy var terminalView: NativeByteStreamTerminalView = {
        var options = TerminalOptions.default
        options.scrollback = 10_000
        let view = NativeByteStreamTerminalView(
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
        view.inputHandler = { [weak self] data in self?.session?.send(data) }
        view.resizeHandler = { [weak self] columns, rows in
            self?.session?.resize(columns: columns, rows: rows)
        }
        view.titleHandler = { [weak self] title in self?.updateTitle(title) }
        view.workingDirectoryHandler = { [weak self] directory in
            self?.updateWorkingDirectory(directory)
        }
        return view
    }()

    init(
        id: UUID = UUID(),
        workingDirectory: String,
        useCase: LocalTerminalUseCase
    ) {
        self.id = id
        self.useCase = useCase
        let normalized = URL(fileURLWithPath: workingDirectory).standardizedFileURL.path
        initialWorkingDirectory = normalized
        self.workingDirectory = normalized
        let directoryName = URL(fileURLWithPath: normalized).lastPathComponent
        title = directoryName.isEmpty ? "终端" : directoryName
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

    func startIfNeeded() {
        guard !hasStarted, state == .idle else { return }
        hasStarted = true
        let token = lifecycle.begin(terminalID: id)
        do {
            let session = try useCase.open(workingDirectory: initialWorkingDirectory)
            self.session = session
            shellName = session.shellName
            observe(session: session, token: token)
            session.start(
                columns: max(terminalView.terminal.cols, 1),
                rows: max(terminalView.terminal.rows, 1)
            )
            guard session.isRunning else {
                eventTask?.cancel()
                eventTask = nil
                self.session = nil
                _ = lifecycle.finish(token)
                session.close()
                state = .failed("无法启动登录 Shell：\(session.shellName)")
                return
            }
            state = .running
        } catch {
            _ = lifecycle.finish(token)
            state = .failed(error.localizedDescription)
        }
    }

    func focus() {
        terminalView.window?.makeFirstResponder(terminalView)
    }

    func interrupt() {
        guard state == .running else { return }
        session?.interrupt()
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
        eventTask?.cancel()
        eventTask = nil
        lifecycle.reset()
        session?.close()
        session = nil
        state = .closed
    }

    private func observe(
        session: any LocalTerminalSessionPort,
        token: LocalTerminalSessionToken
    ) {
        eventTask?.cancel()
        let events = session.events()
        eventTask = Task { [weak self] in
            for await event in events {
                guard !Task.isCancelled,
                      let self,
                      self.lifecycle.accepts(token) else { return }
                switch event {
                case let .output(data):
                    self.terminalView.feed(byteArray: [UInt8](data)[...])
                case let .terminated(exitCode):
                    guard self.lifecycle.finish(token) else { return }
                    self.eventTask = nil
                    self.session = nil
                    self.state = .exited(exitCode)
                    return
                case let .failed(message):
                    guard self.lifecycle.finish(token) else { return }
                    self.eventTask = nil
                    self.session = nil
                    self.state = .failed(message)
                    return
                }
            }
        }
    }

    private func updateTitle(_ value: String) {
        let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines)
        if !normalized.isEmpty { title = normalized }
    }

    private func updateWorkingDirectory(_ value: String?) {
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
}

struct NativeLocalTerminalSurface: NSViewRepresentable {
    @ObservedObject var terminal: NativeLocalTerminalViewModel

    func makeNSView(context: Context) -> NativeByteStreamTerminalView {
        let view = terminal.terminalView
        DispatchQueue.main.async {
            terminal.startIfNeeded()
            terminal.focus()
        }
        return view
    }

    func updateNSView(_ view: NativeByteStreamTerminalView, context: Context) {
        view.nativeForegroundColor = .textColor
        view.nativeBackgroundColor = .textBackgroundColor
    }

    static func dismantleNSView(_ view: NativeByteStreamTerminalView, coordinator: Void) {
        // Switching resources only detaches the renderer. The owning tab
        // explicitly closes its Core session through the feature ViewModel.
    }
}
