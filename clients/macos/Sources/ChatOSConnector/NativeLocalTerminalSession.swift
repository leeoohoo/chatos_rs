import ChatOSCore
import Darwin
import Foundation
import SwiftTerm

public enum NativeLocalTerminalError: LocalizedError, Sendable, Equatable {
    case invalidWorkingDirectory

    public var errorDescription: String? {
        switch self {
        case .invalidWorkingDirectory:
            "本机终端的工作目录不可用。"
        }
    }
}

public struct NativeLocalTerminalSessionProvider: LocalTerminalSessionProviding {
    public init() {}

    public func makeLocalTerminalSession(
        workingDirectory: String
    ) throws -> any LocalTerminalSessionPort {
        try NativeLocalTerminalSession(workingDirectory: workingDirectory)
    }
}

/// Owns the login shell and PTY for one local terminal tab. The App target
/// receives only a byte stream and typed controls; process handles, signals,
/// environment construction, and executable resolution stay in Connector.
public final class NativeLocalTerminalSession: LocalProcessDelegate,
    LocalTerminalSessionPort, @unchecked Sendable {
    public let shellName: String

    private let shellPath: String
    private let workingDirectory: String
    private let environment: [String]
    private let stream: AsyncStream<LocalTerminalEvent>
    private let continuation: AsyncStream<LocalTerminalEvent>.Continuation
    private var viewport = winsize(ws_row: 24, ws_col: 80, ws_xpixel: 0, ws_ypixel: 0)
    private var hasStarted = false
    private var isClosed = false
    private lazy var process = LocalProcess(delegate: self)

    public init(workingDirectory: String) throws {
        let directoryURL = URL(fileURLWithPath: workingDirectory).standardizedFileURL
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(
            atPath: directoryURL.path,
            isDirectory: &isDirectory
        ), isDirectory.boolValue else {
            throw NativeLocalTerminalError.invalidWorkingDirectory
        }

        let configuredShell = ProcessInfo.processInfo.environment["SHELL"] ?? ""
        if configuredShell.hasPrefix("/"),
           FileManager.default.isExecutableFile(atPath: configuredShell) {
            shellPath = configuredShell
        } else {
            shellPath = "/bin/zsh"
        }
        shellName = URL(fileURLWithPath: shellPath).lastPathComponent
        self.workingDirectory = directoryURL.path

        var processEnvironment = ProcessInfo.processInfo.environment
        processEnvironment["TERM"] = "xterm-256color"
        processEnvironment["COLORTERM"] = "truecolor"
        environment = processEnvironment
            .map { "\($0.key)=\($0.value)" }
            .sorted()

        let pair = AsyncStream<LocalTerminalEvent>.makeStream(
            bufferingPolicy: .bufferingNewest(256)
        )
        stream = pair.stream
        continuation = pair.continuation
    }

    public var isRunning: Bool {
        hasStarted && !isClosed && process.running
    }

    public func events() -> AsyncStream<LocalTerminalEvent> {
        stream
    }

    public func start(columns: Int, rows: Int) {
        guard !hasStarted, !isClosed else { return }
        updateViewport(columns: columns, rows: rows)
        hasStarted = true
        process.startProcess(
            executable: shellPath,
            args: ["-l"],
            environment: environment,
            currentDirectory: workingDirectory
        )
        if !process.running {
            continuation.yield(.terminated(exitCode: nil))
            continuation.finish()
        }
    }

    public func send(_ data: Data) {
        guard isRunning, !data.isEmpty else { return }
        let bytes = [UInt8](data)
        process.send(data: bytes[...])
    }

    public func resize(columns: Int, rows: Int) {
        updateViewport(columns: columns, rows: rows)
        guard isRunning, process.childfd >= 0 else { return }
        var size = viewport
        _ = PseudoTerminalHelpers.setWinSize(
            masterPtyDescriptor: process.childfd,
            windowSize: &size
        )
    }

    public func interrupt() {
        send(Data([3]))
    }

    public func close() {
        guard !isClosed else { return }
        isClosed = true
        if process.running, process.shellPid > 0 {
            _ = Darwin.kill(-process.shellPid, SIGHUP)
        }
        process.terminate()
        continuation.finish()
    }

    public func dataReceived(slice: ArraySlice<UInt8>) {
        guard !isClosed, !slice.isEmpty else { return }
        if case .dropped = continuation.yield(.output(Data(slice))) {
            continuation.yield(.failed("终端输出过快，已停止会话以保护内存。"))
            close()
        }
    }

    public func processTerminated(_ source: LocalProcess, exitCode: Int32?) {
        guard !isClosed else { return }
        continuation.yield(.terminated(exitCode: exitCode))
        continuation.finish()
    }

    public func getWindowSize() -> winsize {
        viewport
    }

    deinit {
        close()
    }

    private func updateViewport(columns: Int, rows: Int) {
        viewport.ws_col = UInt16(clamping: min(max(columns, 1), 1_000))
        viewport.ws_row = UInt16(clamping: min(max(rows, 1), 1_000))
    }
}
