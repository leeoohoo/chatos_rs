import Darwin
import Foundation
import SwiftTerm

struct NativeTerminalRelaySnapshot: Sendable, Equatable {
    var data: String
    var baseSequence: UInt64
    var sequence: UInt64
    var truncated: Bool
}

struct NativeTerminalRelayEvent: Sendable {
    var type: String
    var terminalSessionID: String
    var body: NativeJSONValue
}

/// Serializes outbound terminal events through one bounded async stream. If a
/// producer outruns the gateway, sequence recovery uses a snapshot instead of
/// allowing an unbounded number of Swift Tasks to accumulate.
final class NativeTerminalRelayEventPump: @unchecked Sendable {
    let events: AsyncStream<NativeTerminalRelayEvent>
    private let continuation: AsyncStream<NativeTerminalRelayEvent>.Continuation

    init(capacity: Int = 256) {
        let pair = AsyncStream.makeStream(
            of: NativeTerminalRelayEvent.self,
            bufferingPolicy: .bufferingNewest(max(capacity, 1))
        )
        events = pair.stream
        continuation = pair.continuation
    }

    func yield(_ event: NativeTerminalRelayEvent) {
        continuation.yield(event)
    }
}

protocol NativeTerminalRelaySessionProtocol: AnyObject, Sendable {
    var terminalSessionID: String { get }
    var workspaceID: String { get }
    var running: Bool { get }

    func attach(_ eventHandler: @escaping @Sendable (NativeTerminalRelayEvent) -> Void)
    func send(_ data: Data) throws
    func resize(columns: Int, rows: Int)
    func snapshot(maximumLines: Int) -> NativeTerminalRelaySnapshot
    func close()
}

/// A bounded output journal. Sequence numbers identify emitted output chunks,
/// while snapshots retain only the newest four MiB and report when older data
/// was discarded. Interactive input is deliberately never stored here.
final class NativeTerminalRelayOutputJournal: @unchecked Sendable {
    private struct Chunk {
        var sequence: UInt64
        var data: Data
    }

    private let lock = NSLock()
    private let maximumBytes: Int
    private var chunks: [Chunk] = []
    private var byteCount = 0
    private var lastSequence: UInt64 = 0
    private var discardedOutput = false

    init(maximumBytes: Int = 4 * 1_024 * 1_024) {
        precondition(maximumBytes > 0)
        self.maximumBytes = maximumBytes
    }

    func append(_ value: String) -> UInt64 {
        let data = Data(value.utf8)
        return lock.withLock {
            lastSequence &+= 1
            chunks.append(.init(sequence: lastSequence, data: data))
            byteCount += data.count
            while byteCount > maximumBytes, chunks.count > 1 {
                byteCount -= chunks.removeFirst().data.count
                discardedOutput = true
            }
            if byteCount > maximumBytes, let last = chunks.last {
                let suffix = last.data.suffix(maximumBytes)
                chunks = [.init(sequence: last.sequence, data: Data(suffix))]
                byteCount = suffix.count
                discardedOutput = true
            }
            return lastSequence
        }
    }

    func snapshot(maximumLines: Int) -> NativeTerminalRelaySnapshot {
        lock.withLock {
            var retained = Data()
            retained.reserveCapacity(byteCount)
            for chunk in chunks { retained.append(chunk.data) }
            var transportTruncated = false
            if retained.count > 16 * 1_024 {
                retained = Data(retained.suffix(16 * 1_024))
                while retained.first.map({ $0 & 0b1100_0000 == 0b1000_0000 }) == true {
                    retained.removeFirst()
                }
                transportTruncated = true
            }
            var text = String(decoding: retained, as: UTF8.self)
            let lineLimit = min(max(maximumLines, 1), 5_000)
            var omittedLines = false
            var newlineCount = 0
            var start = text.endIndex
            while start > text.startIndex {
                start = text.index(before: start)
                if text[start] == "\n" {
                    newlineCount += 1
                    if newlineCount >= lineLimit {
                        start = text.index(after: start)
                        omittedLines = start > text.startIndex
                        break
                    }
                }
            }
            if start > text.startIndex {
                text = String(text[start...])
            }
            return .init(
                data: text,
                baseSequence: chunks.first?.sequence ?? lastSequence,
                sequence: lastSequence,
                truncated: discardedOutput || transportTruncated || omittedLines
            )
        }
    }
}

private final class NativeTerminalUTF8Decoder: @unchecked Sendable {
    private var pending = Data()

    func decode(_ incoming: Data, flushing: Bool = false) -> String {
        pending.append(incoming)
        guard !pending.isEmpty else { return "" }
        if flushing {
            defer { pending.removeAll(keepingCapacity: false) }
            return String(decoding: pending, as: UTF8.self)
        }

        let heldByteCount = Self.incompleteUTF8SuffixLength(pending)
        let visibleCount = pending.count - heldByteCount
        guard visibleCount > 0 else { return "" }
        let visible = pending.prefix(visibleCount)
        pending = heldByteCount == 0 ? Data() : Data(pending.suffix(heldByteCount))
        return String(decoding: visible, as: UTF8.self)
    }

    private static func incompleteUTF8SuffixLength(_ data: Data) -> Int {
        let bytes = [UInt8](data.suffix(4))
        guard !bytes.isEmpty else { return 0 }
        var leadIndex = bytes.count - 1
        while leadIndex > 0, bytes[leadIndex] & 0b1100_0000 == 0b1000_0000 {
            leadIndex -= 1
        }
        let lead = bytes[leadIndex]
        let expected: Int
        switch lead {
        case 0b1100_0000...0b1101_1111: expected = 2
        case 0b1110_0000...0b1110_1111: expected = 3
        case 0b1111_0000...0b1111_0111: expected = 4
        default: return 0
        }
        let available = bytes.count - leadIndex
        return available < expected ? available : 0
    }
}

final class NativeLocalTerminalRelaySession: NSObject,
    NativeTerminalRelaySessionProtocol,
    LocalProcessDelegate,
    @unchecked Sendable {
    let terminalSessionID: String
    let workspaceID: String
    let workingDirectory: String

    private let callbackQueue: DispatchQueue
    private let lock = NSLock()
    private let journal: NativeTerminalRelayOutputJournal
    private let decoder = NativeTerminalUTF8Decoder()
    private var viewport: winsize
    private var eventHandler: (@Sendable (NativeTerminalRelayEvent) -> Void)?
    private var closed = false
    private lazy var process = LocalProcess(delegate: self, dispatchQueue: callbackQueue)

    init(
        terminalSessionID: String,
        workspaceID: String,
        workingDirectory: String,
        columns: Int,
        rows: Int,
        maximumSnapshotBytes: Int = 4 * 1_024 * 1_024
    ) {
        self.terminalSessionID = terminalSessionID
        self.workspaceID = workspaceID
        self.workingDirectory = workingDirectory
        self.callbackQueue = DispatchQueue(label: "com.chatos.terminal-relay.\(terminalSessionID)")
        self.journal = NativeTerminalRelayOutputJournal(maximumBytes: maximumSnapshotBytes)
        self.viewport = winsize(
            ws_row: UInt16(clamping: min(max(rows, 1), 1_000)),
            ws_col: UInt16(clamping: min(max(columns, 1), 1_000)),
            ws_xpixel: 0,
            ws_ypixel: 0
        )
        super.init()
    }

    var running: Bool {
        lock.withLock { !closed && process.running }
    }

    func start() throws {
        let shell = Self.resolvedShellPath()
        process.startProcess(
            executable: shell,
            args: ["-l"],
            environment: Self.terminalEnvironment(),
            currentDirectory: workingDirectory
        )
        guard process.running else {
            throw NativeTerminalRelaySessionError.startFailed("无法启动登录 Shell：\(shell)")
        }
        emit(type: "terminal_state", body: .object([
            "state": .string("ready"),
            "busy": .bool(false),
            "protocol_version": .number(2),
        ]))
    }

    func attach(_ eventHandler: @escaping @Sendable (NativeTerminalRelayEvent) -> Void) {
        lock.withLock { self.eventHandler = eventHandler }
    }

    func send(_ data: Data) throws {
        guard data.count <= 64 * 1_024 else {
            throw NativeTerminalRelaySessionError.frameTooLarge
        }
        guard running else { throw NativeTerminalRelaySessionError.notRunning }
        guard !data.isEmpty else { return }
        let bytes = [UInt8](data)
        process.send(data: bytes[...])
    }

    func resize(columns: Int, rows: Int) {
        let normalizedColumns = min(max(columns, 1), 1_000)
        let normalizedRows = min(max(rows, 1), 1_000)
        lock.withLock {
            viewport.ws_col = UInt16(normalizedColumns)
            viewport.ws_row = UInt16(normalizedRows)
        }
        guard process.running, process.childfd >= 0 else { return }
        var size = getWindowSize()
        _ = PseudoTerminalHelpers.setWinSize(
            masterPtyDescriptor: process.childfd,
            windowSize: &size
        )
    }

    func snapshot(maximumLines: Int = 500) -> NativeTerminalRelaySnapshot {
        journal.snapshot(maximumLines: maximumLines)
    }

    func close() {
        let shouldClose = lock.withLock {
            if closed { return false }
            closed = true
            return true
        }
        guard shouldClose else { return }
        emit(type: "terminal_state", body: .object([
            "state": .string("closed"),
            "busy": .bool(false),
            "protocol_version": .number(2),
        ]))
        if process.running, process.shellPid > 0 {
            _ = Darwin.kill(-process.shellPid, SIGHUP)
        }
        process.terminate()
    }

    func dataReceived(slice: ArraySlice<UInt8>) {
        guard lock.withLock({ !closed }) else { return }
        let data = Data(slice)
        for offset in stride(from: 0, to: data.count, by: 16 * 1_024) {
            let end = min(offset + 16 * 1_024, data.count)
            let text = decoder.decode(data.subdata(in: offset..<end))
            if !text.isEmpty { emitOutput(text) }
        }
    }

    func processTerminated(_ source: LocalProcess, exitCode: Int32?) {
        guard lock.withLock({ !closed }) else { return }
        let remainder = decoder.decode(Data(), flushing: true)
        if !remainder.isEmpty { emitOutput(remainder) }
        emit(type: "terminal_exit", body: .object([
            "code": exitCode.map { .number(Double($0)) } ?? .null,
        ]))
    }

    func getWindowSize() -> winsize {
        lock.withLock { viewport }
    }

    deinit { close() }

    private func emitOutput(_ text: String) {
        let sequence = journal.append(text)
        emit(type: "terminal_output", body: .object([
            "data": .string(text),
            "sequence": .number(Double(sequence)),
            "protocol_version": .number(2),
        ]))
    }

    private func emit(type: String, body: NativeJSONValue) {
        let handler = lock.withLock { eventHandler }
        handler?(.init(type: type, terminalSessionID: terminalSessionID, body: body))
    }

    private static func resolvedShellPath() -> String {
        let configured = ProcessInfo.processInfo.environment["SHELL"] ?? ""
        if configured.hasPrefix("/"), FileManager.default.isExecutableFile(atPath: configured) {
            return configured
        }
        return "/bin/zsh"
    }

    private static func terminalEnvironment() -> [String] {
        var environment = ProcessInfo.processInfo.environment
        environment["TERM"] = "xterm-256color"
        environment["COLORTERM"] = "truecolor"
        return environment.map { "\($0.key)=\($0.value)" }.sorted()
    }
}

final class NativeRemoteTerminalRelaySession: NSObject,
    NativeTerminalRelaySessionProtocol,
    NativeRemoteTerminalSessionDelegate,
    @unchecked Sendable {
    let terminalSessionID: String
    let workspaceID: String
    let connectionID: String

    private let session: NativeRemoteTerminalSession
    private let lock = NSLock()
    private let journal = NativeTerminalRelayOutputJournal()
    private let decoder = NativeTerminalUTF8Decoder()
    private var eventHandler: (@Sendable (NativeTerminalRelayEvent) -> Void)?
    private var closed = false

    init(
        terminalSessionID: String,
        workspaceID: String,
        connectionID: String,
        session: NativeRemoteTerminalSession
    ) {
        self.terminalSessionID = terminalSessionID
        self.workspaceID = workspaceID
        self.connectionID = connectionID
        self.session = session
        super.init()
        session.delegate = self
    }

    var running: Bool { lock.withLock { !closed && session.running } }

    func start(columns: Int, rows: Int) throws {
        session.start(columns: columns, rows: rows)
        guard session.running else {
            throw NativeTerminalRelaySessionError.startFailed("无法启动远程 SSH PTY")
        }
        emit(type: "terminal_state", body: .object([
            "state": .string("ready"),
            "busy": .bool(false),
            "protocol_version": .number(2),
        ]))
    }

    func attach(_ eventHandler: @escaping @Sendable (NativeTerminalRelayEvent) -> Void) {
        lock.withLock { self.eventHandler = eventHandler }
    }

    func send(_ data: Data) throws {
        guard data.count <= 64 * 1_024 else {
            throw NativeTerminalRelaySessionError.frameTooLarge
        }
        guard running else { throw NativeTerminalRelaySessionError.notRunning }
        session.send(data)
    }

    func resize(columns: Int, rows: Int) {
        session.resize(columns: min(max(columns, 1), 1_000), rows: min(max(rows, 1), 1_000))
    }

    func snapshot(maximumLines: Int = 500) -> NativeTerminalRelaySnapshot {
        journal.snapshot(maximumLines: maximumLines)
    }

    func close() {
        let shouldClose = lock.withLock {
            if closed { return false }
            closed = true
            return true
        }
        guard shouldClose else { return }
        emit(type: "terminal_state", body: .object([
            "state": .string("closed"),
            "busy": .bool(false),
            "protocol_version": .number(2),
        ]))
        session.close()
    }

    func remoteTerminalSession(_ session: NativeRemoteTerminalSession, didReceive data: Data) {
        for offset in stride(from: 0, to: data.count, by: 16 * 1_024) {
            let end = min(offset + 16 * 1_024, data.count)
            let text = decoder.decode(data.subdata(in: offset..<end))
            if !text.isEmpty { emitOutput(text) }
        }
    }

    func remoteTerminalSession(
        _ session: NativeRemoteTerminalSession,
        didTerminateWith exitCode: Int32?
    ) {
        let remainder = decoder.decode(Data(), flushing: true)
        if !remainder.isEmpty { emitOutput(remainder) }
        emit(type: "terminal_exit", body: .object([
            "code": exitCode.map { .number(Double($0)) } ?? .null,
        ]))
    }

    deinit { close() }

    private func emitOutput(_ text: String) {
        let sequence = journal.append(text)
        emit(type: "terminal_output", body: .object([
            "data": .string(text),
            "sequence": .number(Double(sequence)),
            "protocol_version": .number(2),
        ]))
    }

    private func emit(type: String, body: NativeJSONValue) {
        let handler = lock.withLock { eventHandler }
        handler?(.init(type: type, terminalSessionID: terminalSessionID, body: body))
    }
}

enum NativeTerminalRelaySessionError: LocalizedError {
    case startFailed(String)
    case frameTooLarge
    case notRunning

    var errorDescription: String? {
        switch self {
        case let .startFailed(message): message
        case .frameTooLarge: "终端输入帧超过 64 KiB 限制"
        case .notRunning: "终端会话未运行"
        }
    }
}
