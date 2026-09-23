import AppKit
import ChatOSConnector
import ChatOSCore
import Foundation
import SwiftTerm
import SwiftUI

@MainActor
final class NativeRemoteTerminalViewModel: ObservableObject {
    enum State: Equatable {
        case idle
        case connecting
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
    @Published var verificationPrompt: String?
    @Published var verificationCode = ""

    let connectionID: String
    let connectionName: String

    private let sessionProvider: (any NativeRemoteTerminalSessionProviding)?
    private var session: NativeRemoteTerminalSession?
    private var sessionObserver: NativeRemoteTerminalObserver?
    private var connectTask: Task<Void, Never>?

    private(set) lazy var terminalView: NativeRemoteTerminalView = {
        var options = TerminalOptions.default
        options.scrollback = 10_000
        let view = NativeRemoteTerminalView(
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
        connectionID: String,
        connectionName: String,
        initialWorkingDirectory: String,
        sessionProvider: (any NativeRemoteTerminalSessionProviding)?
    ) {
        self.connectionID = connectionID
        self.connectionName = connectionName
        self.title = connectionName
        self.workingDirectory = initialWorkingDirectory
        self.sessionProvider = sessionProvider
    }

    var isConnected: Bool { state == .running }

    var statusTitle: String {
        switch state {
        case .idle: "准备连接"
        case .connecting: "连接中"
        case .running: "已连接"
        case .exited: "连接已退出"
        case .failed: "连接失败"
        case .closed: "已断开"
        }
    }

    var statusColor: SwiftUI.Color {
        switch state {
        case .running: .green
        case .idle: .secondary
        case .connecting: .blue
        case .exited, .closed: .orange
        case .failed: .red
        }
    }

    var failureMessage: String? {
        guard case let .failed(message) = state else { return nil }
        return message
    }

    func startIfNeeded() {
        guard state == .idle else { return }
        connect(verificationCode: nil)
    }

    func submitVerificationCode() {
        let code = verificationCode.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !code.isEmpty else { return }
        verificationPrompt = nil
        verificationCode = ""
        connect(verificationCode: code)
    }

    func cancelVerification() {
        verificationPrompt = nil
        verificationCode = ""
        state = .failed("SSH 二次验证已取消。")
    }

    private func connect(verificationCode: String?) {
        guard state == .idle || verificationCode != nil else { return }
        guard let sessionProvider else {
            state = .failed("当前远程连接服务不支持交互式终端。")
            return
        }

        state = .connecting
        connectTask = Task { [weak self] in
            guard let self else { return }
            do {
                let session = try await sessionProvider.makeRemoteTerminalSession(
                    connectionID: connectionID,
                    verificationCode: verificationCode
                )
                guard !Task.isCancelled, self.state == .connecting else {
                    session.close()
                    return
                }
                let observer = NativeRemoteTerminalObserver(owner: self)
                self.sessionObserver = observer
                self.session = session
                session.delegate = observer
                session.start(
                    columns: max(self.terminalView.terminal.cols, 1),
                    rows: max(self.terminalView.terminal.rows, 1)
                )
                self.state = session.running
                    ? .running
                    : .failed("无法启动本机 OpenSSH。")
                self.focus()
            } catch let challenge as RemoteVerificationChallenge {
                guard !Task.isCancelled else { return }
                self.state = .idle
                self.verificationCode = ""
                self.verificationPrompt = challenge.prompt
            } catch {
                guard !Task.isCancelled else { return }
                self.state = .failed(error.localizedDescription)
            }
        }
    }

    func focus() {
        terminalView.window?.makeFirstResponder(terminalView)
    }

    func reconnect() {
        connectTask?.cancel()
        connectTask = nil
        session?.close()
        session = nil
        sessionObserver = nil
        terminalView.terminal.resetToInitialState()
        title = connectionName
        state = .idle
        startIfNeeded()
    }

    func disconnect() {
        guard state != .closed else { return }
        connectTask?.cancel()
        connectTask = nil
        session?.close()
        session = nil
        sessionObserver = nil
        state = .closed
    }

    func interrupt() {
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

    fileprivate func receive(_ data: Data) {
        guard state != .closed else { return }
        terminalView.feed(byteArray: [UInt8](data)[...])
    }

    fileprivate func processTerminated(exitCode: Int32?) {
        guard state != .closed else { return }
        session = nil
        sessionObserver = nil
        state = .exited(exitCode)
    }

    private func updateSearch() {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        if query.isEmpty {
            terminalView.clearSearch()
        } else {
            terminalView.findNext(query)
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
}

struct NativeRemoteTerminalSurface: NSViewRepresentable {
    @ObservedObject var terminal: NativeRemoteTerminalViewModel

    func makeNSView(context: Context) -> NativeRemoteTerminalView {
        let view = terminal.terminalView
        terminal.startIfNeeded()
        DispatchQueue.main.async { terminal.focus() }
        return view
    }

    func updateNSView(_ view: NativeRemoteTerminalView, context: Context) {
        view.nativeForegroundColor = .textColor
        view.nativeBackgroundColor = .textBackgroundColor
    }
}

private final class NativeRemoteTerminalObserver: NSObject,
    NativeRemoteTerminalSessionDelegate {
    private weak var owner: NativeRemoteTerminalViewModel?

    init(owner: NativeRemoteTerminalViewModel) {
        self.owner = owner
    }

    func remoteTerminalSession(
        _ session: NativeRemoteTerminalSession,
        didReceive data: Data
    ) {
        MainActor.assumeIsolated { [weak owner] in owner?.receive(data) }
    }

    func remoteTerminalSession(
        _ session: NativeRemoteTerminalSession,
        didTerminateWith exitCode: Int32?
    ) {
        MainActor.assumeIsolated { [weak owner] in
            owner?.processTerminated(exitCode: exitCode)
        }
    }
}

final class NativeRemoteTerminalView: TerminalView, @preconcurrency TerminalViewDelegate {
    var inputHandler: ((Data) -> Void)?
    var resizeHandler: ((Int, Int) -> Void)?
    var titleHandler: ((String) -> Void)?
    var workingDirectoryHandler: ((String?) -> Void)?

    override init(frame: CGRect) {
        super.init(frame: frame)
        terminalDelegate = self
    }

    override init(frame: CGRect, font: NSFont? = nil, options: TerminalOptions) {
        super.init(frame: frame, font: font, options: options)
        terminalDelegate = self
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        terminalDelegate = self
    }

    func sizeChanged(source: TerminalView, newCols: Int, newRows: Int) {
        resizeHandler?(newCols, newRows)
    }

    func setTerminalTitle(source: TerminalView, title: String) {
        titleHandler?(title)
    }

    func hostCurrentDirectoryUpdate(source: TerminalView, directory: String?) {
        workingDirectoryHandler?(directory)
    }

    func send(source: TerminalView, data: ArraySlice<UInt8>) {
        inputHandler?(Data(data))
    }

    func scrolled(source: TerminalView, position: Double) {}

    func rangeChanged(source: TerminalView, startY: Int, endY: Int) {}

    func requestOpenLink(source: TerminalView, link: String, params: [String: String]) {
        guard let url = URL(string: link),
              let scheme = url.scheme?.lowercased(),
              ["http", "https", "mailto"].contains(scheme) else {
            return
        }
        NSWorkspace.shared.open(url)
    }

    func clipboardCopy(source: TerminalView, content: Data) {
        guard let value = String(data: content, encoding: .utf8) else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.writeObjects([value as NSString])
    }
}
