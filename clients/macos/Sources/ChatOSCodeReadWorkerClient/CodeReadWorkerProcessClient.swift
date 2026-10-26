import ChatOSCodeReadWorkerProtocol
import Darwin
import Dispatch
import Foundation

public protocol CodeReadWorkerExecuting: Sendable {
    func execute(
        operation: String,
        arguments: [String: CodeReadWorkerJSONValue],
        projectRoot: URL,
        requestCWD: String?,
        defaultToolRoot: String?,
        ripgrepURL: URL?
    ) async throws -> CodeReadWorkerJSONValue
}

public protocol CodeReadWorkerBinaryExecuting: CodeReadWorkerExecuting {
    func execute(
        operation: String,
        arguments: [String: CodeReadWorkerJSONValue],
        binaryPayloads: [Data],
        projectRoot: URL,
        requestCWD: String?,
        defaultToolRoot: String?,
        ripgrepURL: URL?
    ) async throws -> CodeReadWorkerJSONValue
}

public enum CodeReadWorkerProcessClientError: LocalizedError, Equatable {
    case unavailable
    case invalidExecutable
    case launchFailed
    case incompatibleProtocol
    case invalidResponse
    case requestRejected(code: String, message: String)
    case timedOut

    public var errorDescription: String? {
        switch self {
        case .unavailable: "代码读取独立进程不可用。"
        case .invalidExecutable: "代码读取独立进程未通过本机文件校验。"
        case .launchFailed: "无法启动代码读取独立进程。"
        case .incompatibleProtocol: "代码读取独立进程版本与客户端不兼容。"
        case .invalidResponse: "代码读取独立进程返回了无效结果。"
        case let .requestRejected(_, message): message
        case .timedOut: "代码读取超时，已终止对应的独立进程。"
        }
    }
}

public struct CodeReadWorkerProcessClient: CodeReadWorkerBinaryExecuting {
    public static let shared = CodeReadWorkerProcessClient()

    /// The helper lifecycle uses synchronous Process/FileHandle calls. Keep those blocking waits
    /// off Swift's cooperative executor so several independent code reads cannot starve unrelated
    /// async work in the App process.
    private static let blockingWorkQueue = DispatchQueue(
        label: "com.chatos.code-read-worker.blocking-io",
        qos: .userInitiated,
        attributes: .concurrent
    )

    public let executableURL: URL?
    public let timeout: TimeInterval

    public init(executableURL: URL? = nil, timeout: TimeInterval = 60) {
        self.executableURL = executableURL
        self.timeout = timeout
    }

    public func execute(
        operation: String,
        arguments: [String: CodeReadWorkerJSONValue],
        projectRoot: URL,
        requestCWD: String? = nil,
        defaultToolRoot: String? = nil,
        ripgrepURL: URL? = nil
    ) async throws -> CodeReadWorkerJSONValue {
        try await execute(
            operation: operation,
            arguments: arguments,
            binaryPayloads: [],
            projectRoot: projectRoot,
            requestCWD: requestCWD,
            defaultToolRoot: defaultToolRoot,
            ripgrepURL: ripgrepURL
        )
    }

    public func execute(
        operation: String,
        arguments: [String: CodeReadWorkerJSONValue],
        binaryPayloads: [Data],
        projectRoot: URL,
        requestCWD: String? = nil,
        defaultToolRoot: String? = nil,
        ripgrepURL: URL? = nil
    ) async throws -> CodeReadWorkerJSONValue {
        let payloadByteCount = try Self.payloadByteCount(binaryPayloads)
        guard timeout.isFinite,
              (0.05...120).contains(timeout),
              ChatOSCodeReadWorkerProtocol.operations.contains(operation),
              payloadByteCount <= ChatOSCodeReadWorkerProtocol.maximumBinaryPayloadBytes,
              (binaryPayloads.isEmpty || operation == "attachment_authorize_v1") else {
            throw CodeReadWorkerProcessClientError.invalidResponse
        }
        let resolvedExecutable = try executableURL ?? Self.resolveExecutable()
        let controller = CodeReadWorkerProcessController()
        do {
            return try await withTaskCancellationHandler {
                try await withThrowingTaskGroup(of: CodeReadWorkerJSONValue.self) { group in
                    group.addTask {
                        try await withTaskCancellationHandler {
                            try await Self.performBlocking {
                                try controller.run(
                                    executableURL: resolvedExecutable,
                                    operation: operation,
                                    arguments: arguments,
                                    projectRoot: projectRoot,
                                    requestCWD: requestCWD,
                                    defaultToolRoot: defaultToolRoot,
                                    ripgrepURL: ripgrepURL,
                                    binaryPayloads: binaryPayloads,
                                    timeout: timeout
                                )
                            }
                        } onCancel: {
                            controller.terminate()
                        }
                    }
                    group.addTask {
                        try await Task.sleep(
                            nanoseconds: UInt64(max(0.001, timeout) * 1_000_000_000)
                        )
                        controller.terminate(timedOut: true)
                        throw CodeReadWorkerProcessClientError.timedOut
                    }
                    defer { group.cancelAll() }
                    guard let first = try await group.next() else {
                        throw CodeReadWorkerProcessClientError.invalidResponse
                    }
                    return first
                }
            } onCancel: {
                controller.terminate()
            }
        } catch is CancellationError {
            throw CancellationError()
        } catch let error as CodeReadWorkerProcessClientError {
            if Task.isCancelled { throw CancellationError() }
            if controller.didTimeOut { throw CodeReadWorkerProcessClientError.timedOut }
            throw error
        } catch {
            if controller.didTimeOut { throw CodeReadWorkerProcessClientError.timedOut }
            throw CodeReadWorkerProcessClientError.invalidResponse
        }
    }

    private static func payloadByteCount(_ payloads: [Data]) throws -> Int {
        var total = 0
        for payload in payloads {
            let (next, overflow) = total.addingReportingOverflow(payload.count)
            guard !overflow else { throw CodeReadWorkerProcessClientError.invalidResponse }
            total = next
        }
        return total
    }

    private static func resolveExecutable() throws -> URL {
        #if DEBUG
        if let configured = ProcessInfo.processInfo.environment["CHATOS_CODE_READ_WORKER_PATH"]?
            .trimmingCharacters(in: .whitespacesAndNewlines), !configured.isEmpty {
            return URL(fileURLWithPath: configured)
        }
        #endif
        if let bundled = Bundle.main.url(forAuxiliaryExecutable: "ChatOSCodeReadWorker") {
            return bundled
        }
        #if DEBUG
        if let executable = Bundle.main.executableURL {
            let sibling = executable.deletingLastPathComponent()
                .appendingPathComponent("ChatOSCodeReadWorker", isDirectory: false)
            if FileManager.default.isExecutableFile(atPath: sibling.path) { return sibling }
        }
        #endif
        throw CodeReadWorkerProcessClientError.unavailable
    }

    private static func performBlocking(
        _ operation: @escaping @Sendable () throws -> CodeReadWorkerJSONValue
    ) async throws -> CodeReadWorkerJSONValue {
        try await withCheckedThrowingContinuation { continuation in
            blockingWorkQueue.async {
                do {
                    continuation.resume(returning: try operation())
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }
}

private final class CodeReadWorkerProcessController: @unchecked Sendable {
    private enum TerminationReason: Equatable {
        case cancelled
        case timedOut
    }

    private let lock = NSLock()
    private var process: Process?
    private var terminationReason: TerminationReason?
    private var processGroupReady = false

    var didTimeOut: Bool {
        lock.withLock { terminationReason == .timedOut }
    }

    func run(
        executableURL: URL,
        operation: String,
        arguments: [String: CodeReadWorkerJSONValue],
        projectRoot: URL,
        requestCWD: String?,
        defaultToolRoot: String?,
        ripgrepURL: URL?,
        binaryPayloads: [Data],
        timeout: TimeInterval
    ) throws -> CodeReadWorkerJSONValue {
        try validateExecutable(executableURL)
        let canonicalRoot = projectRoot.standardizedFileURL.resolvingSymlinksInPath()
        let rootIdentity = try fileIdentity(canonicalRoot, requireDirectory: true)
        let ripgrepIdentity = try ripgrepURL.map {
            try fileIdentity($0.standardizedFileURL, requireDirectory: false)
        }
        let child = Process()
        let exitSignal = DispatchSemaphore(value: 0)
        child.terminationHandler = { _ in exitSignal.signal() }
        let input = Pipe()
        let output = Pipe()
        guard fcntl(input.fileHandleForWriting.fileDescriptor, F_SETNOSIGPIPE, 1) == 0 else {
            throw CodeReadWorkerProcessClientError.launchFailed
        }
        child.executableURL = executableURL.standardizedFileURL
        child.currentDirectoryURL = executableURL.deletingLastPathComponent()
        child.arguments = ["--parent-pid", String(getpid())]
        child.environment = [
            "LANG": "en_US.UTF-8",
            "TMPDIR": FileManager.default.temporaryDirectory.path,
        ]
        child.standardInput = input
        child.standardOutput = output
        child.standardError = FileHandle.nullDevice
        do {
            try child.run()
        } catch {
            throw CodeReadWorkerProcessClientError.launchFailed
        }
        register(child)
        defer {
            try? input.fileHandleForWriting.close()
            try? output.fileHandleForReading.close()
            if child.isRunning { terminate() }
            clear(child)
        }

        try input.fileHandleForWriting.write(contentsOf: CodeReadWorkerWireCodec.encodeLine(
            CodeReadWorkerHelloRequest(
                clientInstanceID: UUID().uuidString.lowercased(),
                parentProcessID: getpid()
            )
        ))
        let hello = try CodeReadWorkerWireCodec.decode(
            CodeReadWorkerHelloResponse.self,
            from: readLine(output.fileHandleForReading)
        )
        guard hello.type == "hello_response",
              hello.ok,
              hello.selectedProtocolVersion == ChatOSCodeReadWorkerProtocol.maximumVersion,
              hello.workerBuildVersion == ChatOSCodeReadWorkerProtocol.buildVersion,
              hello.operations.contains(operation),
              hello.maximumInlineBytes == ChatOSCodeReadWorkerProtocol.maximumInlineBytes,
              hello.maximumBinaryPayloadBytes
                == ChatOSCodeReadWorkerProtocol.maximumBinaryPayloadBytes,
              !hello.helperInstanceID.isEmpty else {
            throw CodeReadWorkerProcessClientError.incompatibleProtocol
        }
        markProcessGroupReady(child)
        let requestID = UUID().uuidString.lowercased()
        let request = CodeReadWorkerRequest(
            protocolVersion: ChatOSCodeReadWorkerProtocol.maximumVersion,
            requestID: requestID,
            helperInstanceID: hello.helperInstanceID,
            deadlineUnixMilliseconds: Int64((Date().timeIntervalSince1970 + timeout) * 1_000),
            operation: operation,
            arguments: arguments,
            projectRootPath: canonicalRoot.path,
            projectRootIdentity: rootIdentity,
            requestCWD: requestCWD,
            defaultToolRoot: defaultToolRoot,
            ripgrepPath: ripgrepURL?.standardizedFileURL.path,
            ripgrepIdentity: ripgrepIdentity,
            binaryPayloadByteCount: UInt64(binaryPayloads.reduce(0) { $0 + $1.count })
        )
        try input.fileHandleForWriting.write(contentsOf: CodeReadWorkerWireCodec.encodeLine(request))
        try writeBinaryPayloads(binaryPayloads, to: input.fileHandleForWriting.fileDescriptor)
        try input.fileHandleForWriting.close()
        let response = try CodeReadWorkerWireCodec.decode(
            CodeReadWorkerResponse.self,
            from: readLine(output.fileHandleForReading)
        )
        guard response.type == "response", response.requestID == requestID else {
            throw CodeReadWorkerProcessClientError.invalidResponse
        }
        guard response.ok, let result = response.result else {
            throw CodeReadWorkerProcessClientError.requestRejected(
                code: response.error?.code ?? "unknown",
                message: response.error?.message ?? "代码读取独立进程拒绝了请求"
            )
        }
        guard exitSignal.wait(timeout: .now() + 1) == .success,
              child.terminationStatus == EXIT_SUCCESS else {
            throw CodeReadWorkerProcessClientError.invalidResponse
        }
        return result
    }

    private func writeBinaryPayloads(_ payloads: [Data], to descriptor: Int32) throws {
        for payload in payloads {
            try payload.withUnsafeBytes { bytes in
                guard let base = bytes.baseAddress else { return }
                var offset = 0
                while offset < bytes.count {
                    let requested = min(64 * 1_024, bytes.count - offset)
                    let count = Darwin.write(descriptor, base.advanced(by: offset), requested)
                    if count < 0, errno == EINTR { continue }
                    guard count > 0 else {
                        throw CodeReadWorkerProcessClientError.invalidResponse
                    }
                    offset += count
                }
            }
        }
    }

    func terminate(timedOut: Bool = false) {
        lock.lock()
        if terminationReason == nil { terminationReason = timedOut ? .timedOut : .cancelled }
        let process = process
        let groupReady = processGroupReady
        lock.unlock()
        guard let process, process.isRunning else { return }
        signal(processID: process.processIdentifier, signal: SIGTERM, processGroup: groupReady)
        let processID = process.processIdentifier
        DispatchQueue.global(qos: .userInitiated).asyncAfter(deadline: .now() + 0.25) {
            [weak self] in self?.forceKill(processID)
        }
    }

    private func register(_ process: Process) {
        lock.lock()
        self.process = process
        let shouldTerminate = terminationReason != nil
        lock.unlock()
        if shouldTerminate { terminate() }
    }

    private func markProcessGroupReady(_ process: Process) {
        lock.lock()
        if self.process === process { processGroupReady = true }
        let shouldTerminate = terminationReason != nil
        lock.unlock()
        if shouldTerminate { terminate() }
    }

    private func clear(_ process: Process) {
        lock.withLock {
            if self.process === process {
                self.process = nil
                processGroupReady = false
            }
        }
    }

    private func forceKill(_ processID: Int32) {
        lock.lock()
        let shouldKill = process?.processIdentifier == processID && process?.isRunning == true
        let groupReady = processGroupReady
        lock.unlock()
        if shouldKill { signal(processID: processID, signal: SIGKILL, processGroup: groupReady) }
    }

    private func signal(processID: Int32, signal: Int32, processGroup: Bool) {
        if processGroup { _ = Darwin.kill(-processID, signal) }
        _ = Darwin.kill(processID, signal)
    }

    private func validateExecutable(_ url: URL) throws {
        var status = stat()
        let result = url.path.withCString { Darwin.lstat($0, &status) }
        guard url.isFileURL,
              result == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              (status.st_mode & S_IXUSR) != 0 else {
            throw CodeReadWorkerProcessClientError.invalidExecutable
        }
    }

    private func fileIdentity(
        _ url: URL,
        requireDirectory: Bool
    ) throws -> CodeReadWorkerFileIdentity {
        var status = stat()
        let result = url.path.withCString { Darwin.lstat($0, &status) }
        let expected = requireDirectory ? S_IFDIR : S_IFREG
        guard url.isFileURL,
              result == 0,
              (status.st_mode & S_IFMT) == expected else {
            throw CodeReadWorkerProcessClientError.invalidResponse
        }
        return .init(device: UInt64(status.st_dev), inode: UInt64(status.st_ino))
    }

    private func readLine(_ handle: FileHandle) throws -> Data {
        var result = Data()
        var byte: UInt8 = 0
        while result.count <= ChatOSCodeReadWorkerProtocol.maximumInlineBytes {
            let count = Darwin.read(handle.fileDescriptor, &byte, 1)
            if count == 1 {
                if byte == 0x0A { return result }
                result.append(byte)
            } else if count < 0, errno == EINTR {
                continue
            } else {
                throw CodeReadWorkerProcessClientError.invalidResponse
            }
        }
        throw CodeReadWorkerProcessClientError.invalidResponse
    }
}
