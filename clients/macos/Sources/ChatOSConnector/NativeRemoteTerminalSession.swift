import ChatOSCore
import Darwin
import Foundation
import SwiftTerm

public protocol NativeRemoteTerminalSessionProviding: Sendable {
    func makeRemoteTerminalSession(
        connectionID: String,
        verificationCode: String?
    ) async throws -> NativeRemoteTerminalSession
}

public extension NativeRemoteTerminalSessionProviding {
    func makeRemoteTerminalSession(connectionID: String) async throws -> NativeRemoteTerminalSession {
        try await makeRemoteTerminalSession(connectionID: connectionID, verificationCode: nil)
    }
}

public protocol NativeRemoteTerminalSessionDelegate: AnyObject {
    func remoteTerminalSession(
        _ session: NativeRemoteTerminalSession,
        didReceive data: Data
    )

    func remoteTerminalSession(
        _ session: NativeRemoteTerminalSession,
        didTerminateWith exitCode: Int32?
    )
}

/// Owns the OpenSSH process, PTY and short-lived credential helper files for a
/// single interactive remote terminal. The renderer only sees this byte-stream
/// API; SSH passwords and local key paths stay inside ChatOSConnector.
public final class NativeRemoteTerminalSession: LocalProcessDelegate, @unchecked Sendable {
    public weak var delegate: (any NativeRemoteTerminalSessionDelegate)?

    public let connectionID: String
    public let initialWorkingDirectory: String

    private let runtimeDirectory: URL
    private let environment: [String]
    private let arguments: [String]
    private var viewport = winsize(ws_row: 24, ws_col: 80, ws_xpixel: 0, ws_ypixel: 0)
    private var hasStarted = false
    private var isClosed = false

    private lazy var process = LocalProcess(delegate: self)

    var runtimeDirectoryURL: URL { runtimeDirectory }
    var launchArguments: [String] { arguments }

    init(
        connectionID: String,
        draft: RemoteConnectionDraft,
        verificationCode: String? = nil
    ) throws {
        try NativeSSHConnectionTester.validate(draft)

        self.connectionID = connectionID
        self.initialWorkingDirectory = draft.defaultRemotePath?.terminalTrimmedNonEmpty ?? "~"

        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-remote-terminal-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        self.runtimeDirectory = directory

        let config = directory.appendingPathComponent("ssh_config", isDirectory: false)
        let askpass = directory.appendingPathComponent("askpass.sh", isDirectory: false)
        do {
            // The interactive terminal owns the authenticated SSH master. File
            // browsing and command operations can then reuse it without asking
            // the jump host for a second SMS code.
            let controlPath = try NativeOpenSSHClient.persistentControlPath(for: draft)
            try NativeSSHConnectionTester.sshConfig(
                for: draft,
                controlPath: controlPath.path
            )
                .write(to: config, atomically: true, encoding: .utf8)
            try NativeSSHConnectionTester.askpassScript
                .write(to: askpass, atomically: true, encoding: .utf8)
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o600],
                ofItemAtPath: config.path
            )
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o700],
                ofItemAtPath: askpass.path
            )
        } catch {
            try? FileManager.default.removeItem(at: directory)
            throw error
        }
        var processEnvironment = ProcessInfo.processInfo.environment
        processEnvironment["TERM"] = "xterm-256color"
        processEnvironment["COLORTERM"] = "truecolor"
        processEnvironment["SSH_ASKPASS"] = askpass.path
        processEnvironment["SSH_ASKPASS_REQUIRE"] = "force"
        processEnvironment["DISPLAY"] = processEnvironment["DISPLAY"] ?? "chatos:0"
        processEnvironment["CHATOS_SSH_PASSWORD"] = draft.password ?? ""
        processEnvironment["CHATOS_SSH_JUMP_PASSWORD"] = draft.jumpPassword ?? ""
        processEnvironment["CHATOS_SSH_JUMP_HOST"] = draft.jumpHost ?? ""
        processEnvironment["CHATOS_SSH_JUMP_USER"] = draft.jumpUsername ?? ""
        processEnvironment["CHATOS_SSH_VERIFICATION_CODE"] = verificationCode ?? ""
        self.environment = processEnvironment
            .map { "\($0.key)=\($0.value)" }
            .sorted()

        var sshArguments = ["-tt", "-F", config.path, "chatos-target"]
        if let startupCommand = Self.remoteStartupCommand(for: initialWorkingDirectory) {
            sshArguments.append(startupCommand)
        }
        self.arguments = sshArguments
    }

    public var running: Bool {
        hasStarted && !isClosed && process.running
    }

    public func start(columns: Int = 80, rows: Int = 24) {
        guard !hasStarted, !isClosed else { return }
        updateViewport(columns: columns, rows: rows)
        hasStarted = true
        process.startProcess(
            executable: "/usr/bin/ssh",
            args: arguments,
            environment: environment,
            currentDirectory: FileManager.default.homeDirectoryForCurrentUser.path
        )
        if !process.running {
            cleanupRuntimeDirectory()
            delegate?.remoteTerminalSession(self, didTerminateWith: nil)
        }
    }

    public func send(_ data: Data) {
        guard running, !data.isEmpty else { return }
        let bytes = [UInt8](data)
        process.send(data: bytes[...])
    }

    public func resize(columns: Int, rows: Int) {
        updateViewport(columns: columns, rows: rows)
        guard running, process.childfd >= 0 else { return }
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
        cleanupRuntimeDirectory()
    }

    public func dataReceived(slice: ArraySlice<UInt8>) {
        guard !isClosed, !slice.isEmpty else { return }
        delegate?.remoteTerminalSession(self, didReceive: Data(slice))
    }

    public func processTerminated(_ source: LocalProcess, exitCode: Int32?) {
        guard !isClosed else { return }
        cleanupRuntimeDirectory()
        delegate?.remoteTerminalSession(self, didTerminateWith: exitCode)
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

    private func cleanupRuntimeDirectory() {
        try? FileManager.default.removeItem(at: runtimeDirectory)
    }

    private static func remoteStartupCommand(for directory: String) -> String? {
        guard directory != "~" else { return nil }
        let changeDirectory: String
        if directory.hasPrefix("~/") {
            changeDirectory = "\"$HOME\"/\(shellQuote(String(directory.dropFirst(2))))"
        } else {
            changeDirectory = shellQuote(directory)
        }
        return "cd -- \(changeDirectory) && exec \"${SHELL:-/bin/sh}\" -l"
    }

    private static func shellQuote(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }
}

private extension String {
    var terminalTrimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
