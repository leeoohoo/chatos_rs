import Foundation

extension NativeMCPTerminalStore {
    func sameRoot(_ lhs: URL, _ rhs: URL) -> Bool {
        lhs.standardizedFileURL.resolvingSymlinksInPath().path
            == rhs.standardizedFileURL.resolvingSymlinksInPath().path
    }

    func snapshot(_ process: ManagedTerminalProcess) -> NativeJSONValue {
        let output = combinedOutput(process, kinds: ["stdout", "stderr"], maximum: 1_200)
        let busy = process.status != "exited"
        return .object([
            "terminal_id": .string(process.id),
            "process_id": .string(process.id),
            "terminal_name": .string(terminalName(process)),
            "status": .string(process.status),
            "process_status": .string(busy ? "running" : "exited"),
            "busy": .bool(busy),
            "has_session": .bool(true),
            "command": .string(process.command),
            "pid": process.process.processIdentifier > 0 ? .number(Double(process.process.processIdentifier)) : .null,
            "started_at": .string(process.startedAt),
            "uptime_seconds": .null,
            "cwd": .string(displayPath(process.cwd, relativeTo: process.projectRoot)),
            "project_id": .null,
            "last_active_at": .string(process.lastActiveAt),
            "output_preview": .string(output.text),
            "output_tail": .string(output.text),
            "output_tail_chars": .number(Double(output.characters)),
            "exit_code": process.exitCode.map { .number(Double($0)) } ?? .null,
        ])
    }

    func combinedOutput(
        _ process: ManagedTerminalProcess,
        kinds: Set<String>,
        maximum: Int? = nil
    ) -> (text: String, characters: Int, truncated: Bool) {
        let full = process.logs.filter { kinds.contains($0.kind) }.map(\.content).joined()
        let characters = full.count
        let maximum = maximum ?? Self.maximumOutputCharacters
        guard characters > maximum else {
            return (full, characters, process.outputBytesWereTruncated)
        }
        return (String(full.suffix(maximum)), characters, true)
    }

    func terminalName(_ process: ManagedTerminalProcess) -> String {
        let path = displayPath(process.cwd, relativeTo: process.projectRoot)
        return path == "." ? process.projectRoot.lastPathComponent : process.cwd.lastPathComponent
    }

    func displayPath(_ url: URL, relativeTo root: URL) -> String {
        let resolvedURL = url.standardizedFileURL.resolvingSymlinksInPath()
        let resolvedRoot = root.standardizedFileURL.resolvingSymlinksInPath()
        guard resolvedURL.path != resolvedRoot.path else { return "." }
        let prefix = resolvedRoot.path.hasSuffix("/") ? resolvedRoot.path : resolvedRoot.path + "/"
        return resolvedURL.path.hasPrefix(prefix) ? String(resolvedURL.path.dropFirst(prefix.count)) : resolvedURL.path
    }

    func requiredID(_ values: [String: NativeJSONValue]) throws -> String {
        guard let id = values.terminalString("terminal_id")?.trimmingCharacters(in: .whitespacesAndNewlines), !id.isEmpty else {
            throw NativeMCPTerminalError.invalidArguments("缺少参数：terminal_id")
        }
        return id
    }

    func timeoutMilliseconds(_ values: [String: NativeJSONValue]) -> Int {
        if let milliseconds = values.terminalInteger("timeout_ms") {
            return clamp(milliseconds, 1_000, Self.maximumWaitMilliseconds)
        }
        if let seconds = values.terminalInteger("timeout") {
            return clamp(seconds * 1_000, 1_000, Self.maximumWaitMilliseconds)
        }
        return 30_000
    }

    func clamp(_ value: Int, _ minimum: Int, _ maximum: Int) -> Int {
        Swift.min(Swift.max(value, minimum), maximum)
    }

    func resultScope(_ count: Int) -> String {
        count > 1 ? "multiple_terminals" : count == 1 ? "single_terminal" : "no_terminal"
    }

    static func timestamp() -> String { ISO8601DateFormatter().string(from: Date()) }

    static func processReadProperties(defaultLimit: Int) -> [String: NativeJSONValue] {
        [
            "terminal_id": .object(["type": .string("string")]),
            "offset": integerSchema(minimum: 0, maximum: nil),
            "limit": .object([
                "type": .string("integer"),
                "minimum": .number(1),
                "maximum": .number(200),
                "default": .number(Double(defaultLimit)),
            ]),
        ]
    }

    static func integerSchema(minimum: Int, maximum: Int?) -> NativeJSONValue {
        var values: [String: NativeJSONValue] = [
            "type": .string("integer"),
            "minimum": .number(Double(minimum)),
        ]
        if let maximum { values["maximum"] = .number(Double(maximum)) }
        return .object(values)
    }

    static func definition(
        name: String,
        description: String,
        properties: [String: NativeJSONValue],
        required: [String]
    ) -> NativeJSONValue {
        .object([
            "name": .string(name),
            "description": .string(description),
            "inputSchema": .object([
                "type": .string("object"),
                "properties": .object(properties),
                "required": .array(required.map(NativeJSONValue.string)),
                "additionalProperties": .bool(false),
            ]),
        ])
    }
}

final class ManagedTerminalProcess: @unchecked Sendable {
    let id: String
    let sequence: Int
    let command: String
    let cwd: URL
    let projectRoot: URL
    let ownerRunID: String?
    let process: Process
    let input: FileHandle
    let output: Pipe
    let error: Pipe
    var status: String
    var exitCode: Int?
    let startedAt: String
    var lastActiveAt: String
    var logs: [TerminalLog]
    var nextLogOffset: Int
    var retainedLogBytes: Int
    var logsWereTruncated: Bool
    var outputBytesWereTruncated: Bool
    var exitContinuations: [UUID: AsyncStream<Void>.Continuation]
    let stdoutPending = NativeCoalescingProcessOutput(
        maximumBytes: NativeMCPTerminalStore.maximumPendingOutputBytes
    )
    let stderrPending = NativeCoalescingProcessOutput(
        maximumBytes: NativeMCPTerminalStore.maximumPendingOutputBytes
    )
    let stdoutDrain = NativeProcessPipeDrainSignal()
    let stderrDrain = NativeProcessPipeDrainSignal()

    init(
        id: String,
        sequence: Int,
        command: String,
        cwd: URL,
        projectRoot: URL,
        ownerRunID: String?,
        process: Process,
        input: FileHandle,
        output: Pipe,
        error: Pipe,
        status: String,
        exitCode: Int?,
        startedAt: String,
        lastActiveAt: String,
        logs: [TerminalLog],
        nextLogOffset: Int,
        retainedLogBytes: Int,
        logsWereTruncated: Bool,
        outputBytesWereTruncated: Bool,
        exitContinuations: [UUID: AsyncStream<Void>.Continuation] = [:]
    ) {
        self.id = id
        self.sequence = sequence
        self.command = command
        self.cwd = cwd
        self.projectRoot = projectRoot
        self.ownerRunID = ownerRunID
        self.process = process
        self.input = input
        self.output = output
        self.error = error
        self.status = status
        self.exitCode = exitCode
        self.startedAt = startedAt
        self.lastActiveAt = lastActiveAt
        self.logs = logs
        self.nextLogOffset = nextLogOffset
        self.retainedLogBytes = retainedLogBytes
        self.logsWereTruncated = logsWereTruncated
        self.outputBytesWereTruncated = outputBytesWereTruncated
        self.exitContinuations = exitContinuations
    }
}

/// Coordinates process termination with the final pipe-reader callback. A
/// process exit can race the readability handler; waiting for EOF prevents a
/// foreground result from being returned before its last output batch is
/// accounted for.
final class NativeProcessPipeDrainSignal: @unchecked Sendable {
    private let lock = NSLock()
    private var completed = false
    private var continuations: [UUID: AsyncStream<Void>.Continuation] = [:]

    func complete() {
        lock.lock()
        guard !completed else {
            lock.unlock()
            return
        }
        completed = true
        let pending = Array(continuations.values)
        continuations.removeAll()
        lock.unlock()
        for continuation in pending {
            continuation.yield(())
            continuation.finish()
        }
    }

    func wait(timeoutMilliseconds: Int) async -> Bool {
        let changes = stream()
        return await withTaskGroup(of: Bool.self) { group in
            group.addTask {
                for await _ in changes { return true }
                return true
            }
            group.addTask {
                try? await Task.sleep(for: .milliseconds(timeoutMilliseconds))
                return false
            }
            let result = await group.next() ?? false
            group.cancelAll()
            return result
        }
    }

    private func stream() -> AsyncStream<Void> {
        lock.lock()
        if completed {
            lock.unlock()
            return AsyncStream { continuation in
                continuation.yield(())
                continuation.finish()
            }
        }
        let subscriberID = UUID()
        let pair = AsyncStream<Void>.makeStream(bufferingPolicy: .bufferingNewest(1))
        continuations[subscriberID] = pair.continuation
        pair.continuation.onTermination = { [weak self] _ in
            self?.remove(subscriberID)
        }
        lock.unlock()
        return pair.stream
    }

    private func remove(_ subscriberID: UUID) {
        lock.lock()
        continuations.removeValue(forKey: subscriberID)
        lock.unlock()
    }
}

/// Keeps pipe readers non-blocking without creating one actor task per output
/// fragment. Only one delivery task may be outstanding and queued bytes are
/// bounded, so a command that writes faster than the actor can consume cannot
/// create an unbounded task/data backlog.
final class NativeCoalescingProcessOutput: @unchecked Sendable {
    private let lock = NSLock()
    private let maximumBytes: Int
    private var storage = Data()
    private var deliveryScheduled = false
    private var discarded = false

    init(maximumBytes: Int) {
        self.maximumBytes = max(1, maximumBytes)
    }

    /// Returns true only when the caller must schedule a delivery task.
    func append(_ data: Data) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        if !data.isEmpty {
            storage.append(data)
            if storage.count > maximumBytes {
                storage = Data(storage.suffix(maximumBytes))
                discarded = true
            }
        }
        guard !deliveryScheduled, !storage.isEmpty else { return false }
        deliveryScheduled = true
        return true
    }

    func take() -> (data: Data, discarded: Bool)? {
        lock.lock()
        defer { lock.unlock() }
        guard deliveryScheduled else { return nil }
        let result = (storage, discarded)
        storage.removeAll(keepingCapacity: true)
        discarded = false
        deliveryScheduled = false
        return result
    }
}

struct TerminalLog: Sendable {
    let offset: Int
    let kind: String
    let content: String
    let byteCount: Int
    let createdAt: String

    var jsonValue: NativeJSONValue {
        .object([
            "offset": .number(Double(offset)),
            "kind": .string(kind),
            "content": .string(content),
            "created_at": .string(createdAt),
        ])
    }
}

enum NativeMCPTerminalError: LocalizedError {
    case unsupportedTool(String)
    case invalidArguments(String)
    case launchFailed(String)
    case processNotFound
    case processExited
    case writeFailed(String)

    var errorDescription: String? {
        switch self {
        case let .unsupportedTool(name): "不支持的终端工具：\(name)"
        case let .invalidArguments(message): message
        case let .launchFailed(message): "命令启动失败：\(message)"
        case .processNotFound: "当前项目中没有找到该命令进程"
        case .processExited: "命令进程已经结束"
        case let .writeFailed(message): "写入命令进程失败：\(message)"
        }
    }
}

extension Dictionary where Key == String, Value == NativeJSONValue {
    func terminalString(_ key: String) -> String? {
        guard case let .string(value)? = self[key] else { return nil }
        return value
    }

    func terminalBool(_ key: String) -> Bool? {
        guard case let .bool(value)? = self[key] else { return nil }
        return value
    }

    func terminalInteger(_ key: String) -> Int? {
        guard case let .number(value)? = self[key] else { return nil }
        return Int(value)
    }

    func terminalArray(_ key: String) -> [NativeJSONValue] {
        guard case let .array(value)? = self[key] else { return [] }
        return value
    }
}

extension NativeJSONValue {
    var content: String? {
        guard case let .object(values) = self, case let .string(content)? = values["content"] else { return nil }
        return content
    }
}
