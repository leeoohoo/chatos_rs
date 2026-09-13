// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Darwin
import Foundation

public let localAgentHostLaunchProtocolVersion: UInt32 = 4
private let localAgentHostStorageUnavailableExitStatus: Int32 = 75

public enum NativeLocalAgentHostExitCause: Equatable, Sendable {
    case storageUnavailable
    case unexpected(status: Int32)
}

public struct NativeLocalAgentHostExit: Equatable, Sendable {
    public let status: Int32
    public let cause: NativeLocalAgentHostExitCause
    public let detail: String
}

public enum NativeLocalAgentHostLaunchError: Error, Equatable, Sendable {
    case invalidConfiguration(String)
    case untrustedExecutable
    case processLaunchFailed(String)
    case launchFrameWriteFailed
    case readyTimeout
    case readyFrameInvalid
    case processExitedBeforeReady(status: Int32, detail: String)
    case readyProtocolMismatch(UInt32)
    case readyLaunchMismatch
    case readyProcessMismatch
    case readyEndpointMismatch
}

extension NativeLocalAgentHostLaunchError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case let .invalidConfiguration(message): message
        case .untrustedExecutable: "本地 Agent Host 可执行文件未通过身份校验"
        case let .processLaunchFailed(message): "本地 Agent Host 启动失败：\(message)"
        case .launchFrameWriteFailed: "本地 Agent Host 启动配置写入失败"
        case .readyTimeout: "本地 Agent Host 启动握手超时"
        case .readyFrameInvalid: "本地 Agent Host 返回了无效启动握手"
        case let .processExitedBeforeReady(status, detail):
            "本地 Agent Host 在启动握手前退出（状态码：\(status)）：\(detail)"
        case let .readyProtocolMismatch(version): "本地 Agent Host 启动协议不匹配（\(version)）"
        case .readyLaunchMismatch: "本地 Agent Host 启动标识不匹配"
        case .readyProcessMismatch: "本地 Agent Host 进程身份不匹配"
        case .readyEndpointMismatch: "本地 Agent Host IPC 地址不匹配"
        }
    }
}

public struct NativeLocalAgentHostLaunchConfiguration: Sendable {
    public let executableURL: URL
    public let launchID: String
    public let expectedClientEndpoint: String
    public let launchMaterial: NativeLocalAgentHostLaunchMaterial
    public let secretMaterial: NativeLocalAgentHostLaunchMaterial
    public let readyTimeout: Duration

    public init(
        executableURL: URL,
        launchID: String,
        expectedClientEndpoint: String,
        launchRequestJSON: Data,
        secretFrameJSON: Data,
        readyTimeout: Duration = .seconds(30)
    ) throws {
        guard executableURL.isFileURL,
              executableURL.path.hasPrefix("/"),
              !launchID.isEmpty,
              launchID == launchID.trimmingCharacters(in: .whitespacesAndNewlines),
              !expectedClientEndpoint.isEmpty,
              expectedClientEndpoint == expectedClientEndpoint.trimmingCharacters(in: .whitespacesAndNewlines),
              !launchRequestJSON.isEmpty,
              launchRequestJSON.count <= 1024 * 1024,
              !secretFrameJSON.isEmpty,
              secretFrameJSON.count <= 512 * 1024,
              readyTimeout > .zero,
              readyTimeout <= .seconds(120)
        else {
            throw NativeLocalAgentHostLaunchError.invalidConfiguration(
                "本地 Agent Host 启动配置无效"
            )
        }
        self.executableURL = executableURL
        self.launchID = launchID
        self.expectedClientEndpoint = expectedClientEndpoint
        self.launchMaterial = try NativeLocalAgentHostLaunchMaterial(launchRequestJSON)
        self.secretMaterial = try NativeLocalAgentHostLaunchMaterial(secretFrameJSON)
        self.readyTimeout = readyTimeout
    }
}

public final class NativeLocalAgentHostLaunchMaterial: @unchecked Sendable {
    private let lock = NSLock()
    private var requestJSON: Data?

    public init(_ requestJSON: Data) throws {
        guard !requestJSON.isEmpty, requestJSON.count <= 1024 * 1024 else {
            throw NativeLocalAgentHostLaunchError.invalidConfiguration(
                "本地 Agent Host 启动配置大小无效"
            )
        }
        self.requestJSON = requestJSON
    }

    deinit {
        lock.lock()
        zeroize(&requestJSON)
        lock.unlock()
    }

    fileprivate func consume() throws -> Data {
        lock.lock()
        defer { lock.unlock() }
        guard let data = requestJSON else {
            throw NativeLocalAgentHostLaunchError.invalidConfiguration(
                "本地 Agent Host 启动配置已经使用"
            )
        }
        requestJSON = nil
        return data
    }

    func snapshotForTesting() throws -> Data {
        lock.lock()
        defer { lock.unlock() }
        guard let requestJSON else {
            throw NativeLocalAgentHostLaunchError.invalidConfiguration(
                "本地 Agent Host 启动配置已经使用"
            )
        }
        return requestJSON
    }

    private func zeroize(_ value: inout Data?) {
        let count = value?.count ?? 0
        value?.resetBytes(in: 0..<count)
        value = nil
    }
}

public struct NativeLocalAgentHostReady: Decodable, Equatable, Sendable {
    public let protocolVersion: UInt32
    public let launchID: String
    public let processID: UInt32
    public let clientEndpoint: String

    private enum CodingKeys: String, CodingKey {
        case protocolVersion = "protocol_version"
        case launchID = "launch_id"
        case processID = "process_id"
        case clientEndpoint = "client_endpoint"
    }
}

public final class NativeLocalAgentHostProcess: @unchecked Sendable {
    public let ready: NativeLocalAgentHostReady
    private let process: Process?
    private let standardErrorCollector: BoundedStandardErrorCollector?
    private let exitTask: Task<Int32, Never>?
    private let attachedProcessID: pid_t?
    private let attachedIdentityVerifier: (@Sendable (pid_t) throws -> Void)?

    fileprivate init(
        process: Process,
        ready: NativeLocalAgentHostReady,
        standardErrorCollector: BoundedStandardErrorCollector,
        exitTask: Task<Int32, Never>
    ) {
        self.process = process
        self.ready = ready
        self.standardErrorCollector = standardErrorCollector
        self.exitTask = exitTask
        self.attachedProcessID = nil
        self.attachedIdentityVerifier = nil
    }

    init(
        attachedProcessID: pid_t,
        clientEndpoint: String,
        identityVerifier: @escaping @Sendable (pid_t) throws -> Void
    ) {
        self.ready = NativeLocalAgentHostReady(
            protocolVersion: localAgentHostLaunchProtocolVersion,
            launchID: "attached-\(attachedProcessID)",
            processID: UInt32(attachedProcessID),
            clientEndpoint: clientEndpoint
        )
        self.process = nil
        self.standardErrorCollector = nil
        self.exitTask = nil
        self.attachedProcessID = attachedProcessID
        self.attachedIdentityVerifier = identityVerifier
    }

    public var isRunning: Bool {
        if let process { return process.isRunning }
        return attachedProcessIsRunning()
    }

    public func terminate() {
        if let process {
            guard process.isRunning else { return }
            process.terminate()
            return
        }
        guard let attachedProcessID, attachedProcessIsRunning() else { return }
        _ = Darwin.kill(attachedProcessID, SIGTERM)
    }

    /// Stops the Host before account credentials are revoked or a replacement
    /// process is launched. A wedged native dependency cannot keep the old
    /// account process alive indefinitely after it ignores SIGTERM.
    public func stop(gracePeriod: Duration = .seconds(2)) async -> Int32 {
        guard let process, let exitTask else {
            return await stopAttachedProcess(gracePeriod: gracePeriod)
        }
        if process.isRunning {
            process.terminate()
            let clock = ContinuousClock()
            let deadline = clock.now.advanced(by: gracePeriod)
            while process.isRunning, clock.now < deadline {
                try? await Task.sleep(for: .milliseconds(20))
            }
        }
        if process.isRunning {
            _ = Darwin.kill(process.processIdentifier, SIGKILL)
        }
        return await exitTask.value
    }

    public func waitForExit() async -> NativeLocalAgentHostExit {
        guard let exitTask, let standardErrorCollector else {
            while attachedProcessIsRunning() {
                if Task.isCancelled {
                    return NativeLocalAgentHostExit(
                        status: -1,
                        cause: .unexpected(status: -1),
                        detail: "已停止监测附着的本地 Agent Host"
                    )
                }
                try? await Task.sleep(for: .milliseconds(250))
            }
            return NativeLocalAgentHostExit(
                status: -1,
                cause: .unexpected(status: -1),
                detail: "附着的本地 Agent Host 已退出"
            )
        }
        let status = await exitTask.value
        let detail = await standardErrorCollector.finish()
        return NativeLocalAgentHostExit(
            status: status,
            cause: status == localAgentHostStorageUnavailableExitStatus
                ? .storageUnavailable
                : .unexpected(status: status),
            detail: detail
        )
    }

    private func attachedProcessIsRunning() -> Bool {
        guard let processID = attachedProcessID,
              let identityVerifier = attachedIdentityVerifier
        else { return false }
        let signalResult = Darwin.kill(processID, 0)
        guard signalResult == 0 || errno == EPERM else { return false }
        do {
            try identityVerifier(processID)
            return true
        } catch {
            return false
        }
    }

    private func stopAttachedProcess(gracePeriod: Duration) async -> Int32 {
        guard let processID = attachedProcessID else { return -1 }
        if attachedProcessIsRunning() {
            _ = Darwin.kill(processID, SIGTERM)
            let clock = ContinuousClock()
            let deadline = clock.now.advanced(by: gracePeriod)
            while attachedProcessIsRunning(), clock.now < deadline {
                try? await Task.sleep(for: .milliseconds(20))
            }
        }
        if attachedProcessIsRunning() {
            _ = Darwin.kill(processID, SIGKILL)
        }
        return -1
    }
}

/// Launches the bundled Rust Host without placing any credential in argv,
/// environment variables, preferences, a temporary file, or the ordinary
/// launch frame. After validating the executable, the launcher sends a second,
/// bounded and correlated secret frame over the inherited anonymous stdin
/// pipe; stdout is accepted only as the correlated ready frame.
public struct NativeLocalAgentHostProcessLauncher: Sendable {
    private let identityVerifier: @Sendable (URL) throws -> Void

    public init() {
        identityVerifier = { url in
            try NativeLocalAgentHostIdentity.validate(executableURL: url)
        }
    }

    init(testingIdentityVerifier: @escaping @Sendable (URL) throws -> Void) {
        identityVerifier = testingIdentityVerifier
    }

    public func launch(
        _ configuration: NativeLocalAgentHostLaunchConfiguration
    ) async throws -> NativeLocalAgentHostProcess {
        try verifyExecutable(configuration.executableURL)
        let process = Process()
        let input = Pipe()
        let output = Pipe()
        let standardError = Pipe()
        process.executableURL = configuration.executableURL
        process.arguments = []
        process.environment = ["LANG": "en_US.UTF-8"]
        process.currentDirectoryURL = URL(fileURLWithPath: "/", isDirectory: true)
        process.standardInput = input
        process.standardOutput = output
        process.standardError = standardError
        let exitObserver = NativeProcessExitObserver()
        process.terminationHandler = { process in
            exitObserver.finish(status: process.terminationStatus)
        }
        do {
            try process.run()
        } catch {
            input.fileHandleForReading.closeFile()
            input.fileHandleForWriting.closeFile()
            output.fileHandleForReading.closeFile()
            output.fileHandleForWriting.closeFile()
            standardError.fileHandleForReading.closeFile()
            standardError.fileHandleForWriting.closeFile()
            throw NativeLocalAgentHostLaunchError.processLaunchFailed(error.localizedDescription)
        }
        // Foundation can publish `isRunning == false` before a concurrent
        // `waitUntilExit()` returns. Observe the one termination callback set
        // before launch and fan that durable result to every async waiter.
        let exitTask = Task.detached(priority: .utility) {
            await exitObserver.wait()
        }
        input.fileHandleForReading.closeFile()
        output.fileHandleForWriting.closeFile()
        standardError.fileHandleForWriting.closeFile()
        let standardErrorCollector = BoundedStandardErrorCollector(
            handle: standardError.fileHandleForReading
        )

        do {
            var launchRequestJSON = try configuration.launchMaterial.consume()
            defer { launchRequestJSON.resetBytes(in: 0..<launchRequestJSON.count) }
            var secretFrameJSON = try configuration.secretMaterial.consume()
            defer { secretFrameJSON.resetBytes(in: 0..<secretFrameJSON.count) }
            try writeLaunchFrame(
                launchRequestJSON,
                to: input.fileHandleForWriting
            )
            try writeLaunchFrame(
                secretFrameJSON,
                to: input.fileHandleForWriting
            )
            do {
                try input.fileHandleForWriting.close()
            } catch {
                throw NativeLocalAgentHostLaunchError.launchFrameWriteFailed
            }
            let ready = try await readReadyFrame(
                from: output.fileHandleForReading,
                timeout: configuration.readyTimeout
            )
            try validateReady(ready, configuration: configuration, process: process)
            return NativeLocalAgentHostProcess(
                process: process,
                ready: ready,
                standardErrorCollector: standardErrorCollector,
                exitTask: exitTask
            )
        } catch {
            input.fileHandleForWriting.closeFile()
            output.fileHandleForReading.closeFile()
            let exitedBeforeReady: Bool
            if case NativeLocalAgentHostLaunchError.readyFrameInvalid = error {
                // EOF can arrive a few scheduler ticks before Process publishes
                // its terminal state. Give an already-exiting Host a short,
                // bounded opportunity to expose its real status and stderr;
                // a Host that merely closed stdout remains a protocol failure.
                exitedBeforeReady = await waitForNativeProcessExit(
                    process,
                    timeout: .milliseconds(250)
                )
            } else {
                exitedBeforeReady = !process.isRunning
            }
            let status = await stopNativeProcess(
                process,
                exitTask: exitTask,
                gracePeriod: .milliseconds(500)
            )
            let detail = await standardErrorCollector.finish()
            if case NativeLocalAgentHostLaunchError.readyFrameInvalid = error,
               exitedBeforeReady
            {
                throw NativeLocalAgentHostLaunchError.processExitedBeforeReady(
                    status: status,
                    detail: detail
                )
            }
            throw error
        }
    }

    private func verifyExecutable(_ url: URL) throws {
        var metadata = stat()
        guard lstat(url.path, &metadata) == 0,
              metadata.st_mode & S_IFMT == S_IFREG,
              metadata.st_uid == 0 || metadata.st_uid == geteuid(),
              metadata.st_mode & 0o022 == 0,
              access(url.path, X_OK) == 0
        else {
            throw NativeLocalAgentHostLaunchError.untrustedExecutable
        }
        do {
            try identityVerifier(url)
        } catch {
            throw NativeLocalAgentHostLaunchError.untrustedExecutable
        }
    }

    private func writeLaunchFrame(_ body: Data, to handle: FileHandle) throws {
        var length = UInt32(body.count).bigEndian
        do {
            try withUnsafeBytes(of: &length) { bytes in
                try handle.write(contentsOf: Data(bytes))
            }
            try handle.write(contentsOf: body)
        } catch {
            throw NativeLocalAgentHostLaunchError.launchFrameWriteFailed
        }
    }

    private func readReadyFrame(
        from handle: FileHandle,
        timeout: Duration
    ) async throws -> NativeLocalAgentHostReady {
        let reader = BlockingReadyReader(handle: handle)
        return try await withThrowingTaskGroup(of: NativeLocalAgentHostReady.self) { group in
            group.addTask { try reader.read() }
            group.addTask {
                try await Task.sleep(for: timeout)
                reader.timeout()
                throw NativeLocalAgentHostLaunchError.readyTimeout
            }
            guard let first = try await group.next() else {
                throw NativeLocalAgentHostLaunchError.readyFrameInvalid
            }
            group.cancelAll()
            reader.close()
            return first
        }
    }

    private func validateReady(
        _ ready: NativeLocalAgentHostReady,
        configuration: NativeLocalAgentHostLaunchConfiguration,
        process: Process
    ) throws {
        guard ready.protocolVersion == localAgentHostLaunchProtocolVersion else {
            throw NativeLocalAgentHostLaunchError.readyProtocolMismatch(ready.protocolVersion)
        }
        guard ready.launchID == configuration.launchID else {
            throw NativeLocalAgentHostLaunchError.readyLaunchMismatch
        }
        guard ready.processID == UInt32(process.processIdentifier) else {
            throw NativeLocalAgentHostLaunchError.readyProcessMismatch
        }
        guard ready.clientEndpoint == configuration.expectedClientEndpoint else {
            throw NativeLocalAgentHostLaunchError.readyEndpointMismatch
        }
    }
}

private final class NativeProcessExitObserver: @unchecked Sendable {
    private let lock = NSLock()
    private var status: Int32?
    private var continuation: CheckedContinuation<Int32, Never>?

    func wait() async -> Int32 {
        await withCheckedContinuation { continuation in
            lock.lock()
            if let status {
                lock.unlock()
                continuation.resume(returning: status)
                return
            }
            precondition(self.continuation == nil)
            self.continuation = continuation
            lock.unlock()
        }
    }

    func finish(status: Int32) {
        lock.lock()
        guard self.status == nil else {
            lock.unlock()
            return
        }
        self.status = status
        let continuation = self.continuation
        self.continuation = nil
        lock.unlock()
        continuation?.resume(returning: status)
    }
}

private final class BoundedStandardErrorCollector: @unchecked Sendable {
    fileprivate static let byteLimit = 2_048
    private let reader: Task<Data, Never>

    init(handle: FileHandle) {
        reader = Task.detached(priority: .utility) {
            var captured = Data()
            while true {
                let chunk: Data?
                do {
                    chunk = try handle.read(upToCount: 4_096)
                } catch {
                    break
                }
                guard let chunk, !chunk.isEmpty else { break }
                if captured.count < Self.byteLimit {
                    captured.append(chunk.prefix(Self.byteLimit - captured.count))
                }
            }
            try? handle.close()
            return captured
        }
    }

    func finish() async -> String {
        let data = await reader.value
        let decoded = String(decoding: data, as: UTF8.self)
        let printable = String(decoded.unicodeScalars.filter { scalar in
            scalar.value >= 0x20 && scalar.value != 0x7F
        })
        let normalized = printable
            .split(whereSeparator: { $0.isWhitespace })
            .joined(separator: " ")
        let sanitized = sanitizeHostDiagnostic(normalized)
        return sanitized.isEmpty ? "Host 未返回诊断信息" : sanitized
    }
}

private func sanitizeHostDiagnostic(_ value: String) -> String {
    var sanitized = value
    let patterns: [(String, String)] = [
        (
            #"(?i)\b(?:authorization|x-access-token|access[_-]?token|token|password|passwd|pwd)\s*[:=]\s*[^\s,;]+"#,
            "credential=[REDACTED]"
        ),
        (
            #"(?i)\b(?:https?|postgres(?:ql)?)://[^\s]+"#,
            "[REDACTED_URL]"
        ),
    ]
    for (pattern, replacement) in patterns {
        guard let expression = try? NSRegularExpression(pattern: pattern) else { continue }
        let range = NSRange(sanitized.startIndex..., in: sanitized)
        sanitized = expression.stringByReplacingMatches(
            in: sanitized,
            range: range,
            withTemplate: replacement
        )
    }
    var result = ""
    result.reserveCapacity(min(sanitized.utf8.count, BoundedStandardErrorCollector.byteLimit))
    var byteCount = 0
    for character in sanitized {
        let characterBytes = String(character).utf8.count
        guard byteCount + characterBytes <= BoundedStandardErrorCollector.byteLimit else { break }
        result.append(character)
        byteCount += characterBytes
    }
    return result
}

private func waitForNativeProcessExit(_ process: Process, timeout: Duration) async -> Bool {
    let clock = ContinuousClock()
    let deadline = clock.now.advanced(by: timeout)
    while process.isRunning, clock.now < deadline {
        try? await Task.sleep(for: .milliseconds(10))
    }
    return !process.isRunning
}

private func stopNativeProcess(
    _ process: Process,
    exitTask: Task<Int32, Never>,
    gracePeriod: Duration
) async -> Int32 {
    if process.isRunning {
        process.terminate()
        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: gracePeriod)
        while process.isRunning, clock.now < deadline {
            try? await Task.sleep(for: .milliseconds(20))
        }
    }
    if process.isRunning {
        _ = Darwin.kill(process.processIdentifier, SIGKILL)
    }
    return await exitTask.value
}

private final class BlockingReadyReader: @unchecked Sendable {
    private let handle: FileHandle
    private let lock = NSLock()
    private var closed = false
    private var timedOut = false

    init(handle: FileHandle) {
        self.handle = handle
    }

    func read() throws -> NativeLocalAgentHostReady {
        let lengthData = try readExactly(4)
        let length = lengthData.reduce(UInt32(0)) { value, byte in
            (value << 8) | UInt32(byte)
        }
        guard length > 0, length <= 1024 * 1024 else {
            throw NativeLocalAgentHostLaunchError.readyFrameInvalid
        }
        let body = try readExactly(Int(length))
        guard let ready = try? JSONDecoder().decode(NativeLocalAgentHostReady.self, from: body)
        else {
            throw NativeLocalAgentHostLaunchError.readyFrameInvalid
        }
        return ready
    }

    func close() {
        lock.lock()
        defer { lock.unlock() }
        guard !closed else { return }
        closed = true
        try? handle.close()
    }

    func timeout() {
        lock.lock()
        timedOut = true
        guard !closed else {
            lock.unlock()
            return
        }
        closed = true
        lock.unlock()
        try? handle.close()
    }

    private func readExactly(_ count: Int) throws -> Data {
        var data = Data()
        while data.count < count {
            let chunk: Data?
            do {
                chunk = try handle.read(upToCount: count - data.count)
            } catch {
                throw readFailure()
            }
            guard let chunk, !chunk.isEmpty else {
                throw readFailure()
            }
            data.append(chunk)
        }
        return data
    }

    private func readFailure() -> NativeLocalAgentHostLaunchError {
        lock.lock()
        defer { lock.unlock() }
        return timedOut ? .readyTimeout : .readyFrameInvalid
    }
}
