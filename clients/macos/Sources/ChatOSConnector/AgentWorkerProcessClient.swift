import ChatOSAgentRuntime
import ChatOSAgentWorkerProtocol
import Darwin
import Foundation

public enum AgentWorkerProcessClientError: LocalizedError, Equatable {
    case unavailable
    case invalidExecutable
    case launchFailed
    case incompatibleProtocol
    case invalidResponse
    case workerFailed(String)
    case timedOut

    public var errorDescription: String? {
        switch self {
        case .unavailable: "Agent 独立执行进程不可用。"
        case .invalidExecutable: "Agent 独立执行进程未通过本机文件校验。"
        case .launchFailed: "无法启动 Agent 独立执行进程。"
        case .incompatibleProtocol: "Agent 独立执行进程与客户端协议不兼容。"
        case .invalidResponse: "Agent 独立执行进程返回了无效结果。"
        case let .workerFailed(detail): "Agent 独立执行进程失败：\(detail)"
        case .timedOut: "Agent 独立执行进程超时，已被终止。"
        }
    }
}

/// Launches exactly one helper process for one Agent Run. Model, Memory, tools and SQLite remain
/// parent-owned capabilities and are invoked through the versioned pipe protocol; the worker owns
/// the orchestration loop, timeout state and checkpoint transitions for only this Run.
public struct AgentWorkerProcessClient: AgentRuntimeExecuting, Sendable {
    public typealias ProcessObserver = @Sendable (_ processID: Int32) -> Void

    public let executableURL: URL?
    public let shutdownGraceSeconds: TimeInterval
    private let processObserver: ProcessObserver?

    public init(
        executableURL: URL? = nil,
        shutdownGraceSeconds: TimeInterval = 30,
        processObserver: ProcessObserver? = nil
    ) {
        self.executableURL = executableURL
        self.shutdownGraceSeconds = shutdownGraceSeconds
        self.processObserver = processObserver
    }

    public func executeRun(
        checkpoint: AgentRunCheckpoint,
        scope: String,
        policy: AgentRunPolicy,
        model: any AgentModelClient,
        tools: [AgentToolDefinition],
        execute: @escaping AgentRuntime.Executor,
        completionCheck: @escaping AgentRuntime.CompletionCheck,
        contextProvider: (any AgentContextProviding)?,
        record: @escaping AgentRuntime.Recorder
    ) async throws -> AgentRunCheckpoint {
        try policy.validate()
        guard checkpoint.scope == scope,
              checkpoint.memory != nil ? contextProvider != nil : contextProvider == nil,
              shutdownGraceSeconds.isFinite,
              (1...120).contains(shutdownGraceSeconds) else {
            throw AgentWorkerProcessClientError.invalidResponse
        }
        let resolvedExecutable = try executableURL ?? Self.resolveExecutable()
        try Self.validateExecutable(resolvedExecutable)
        let controller = try AgentWorkerProcessController(
            executableURL: resolvedExecutable,
            parentProcessID: getpid()
        )
        processObserver?(controller.processIdentifier)
        let timeout = Double(policy.runTimeoutSeconds) + shutdownGraceSeconds
        return try await withTaskCancellationHandler {
            do {
                return try await withAgentTimeout(seconds: timeout) {
                    try await Self.runProtocol(
                        controller: controller,
                        checkpoint: checkpoint,
                        scope: scope,
                        policy: policy,
                        model: model,
                        tools: tools,
                        execute: execute,
                        completionCheck: completionCheck,
                        contextProvider: contextProvider,
                        record: record
                    )
                }
            } catch let error as AgentRuntimeError {
                if case .timeout = error {
                    controller.terminate()
                    throw AgentWorkerProcessClientError.timedOut
                }
                throw error
            }
        } onCancel: {
            controller.terminate()
        }
    }

    private static func runProtocol(
        controller: AgentWorkerProcessController,
        checkpoint: AgentRunCheckpoint,
        scope: String,
        policy: AgentRunPolicy,
        model: any AgentModelClient,
        tools: [AgentToolDefinition],
        execute: @escaping AgentRuntime.Executor,
        completionCheck: @escaping AgentRuntime.CompletionCheck,
        contextProvider: (any AgentContextProviding)?,
        record: @escaping AgentRuntime.Recorder
    ) async throws -> AgentRunCheckpoint {
        let nonce = UUID()
        try await controller.send(.hello(.init(
            parentProcessID: getpid(),
            runID: checkpoint.id,
            nonce: nonce
        )))
        guard case let .helloResponse(hello) = try await controller.receive(),
              hello.selectedProtocolVersion == ChatOSAgentWorkerProtocol.maximumVersion,
              hello.runID == checkpoint.id,
              hello.nonce == nonce else {
            throw AgentWorkerProcessClientError.incompatibleProtocol
        }
        try await controller.send(.start(.init(
            checkpoint: checkpoint,
            scope: scope,
            policy: policy,
            tools: tools,
            hasContextProvider: contextProvider != nil,
            modelUsesServerSideCompaction: model.usesServerSideCompaction
        )))

        while true {
            try Task.checkCancellation()
            switch try await controller.receive() {
            case let .modelRequest(request):
                do {
                    let forwarder = AgentWorkerModelEventForwarder(
                        controller: controller,
                        requestID: request.id
                    )
                    let message = try await model.stream(
                        messages: request.messages,
                        tools: request.tools,
                        timeout: request.timeout,
                        onEvent: { event in await forwarder.forward(event) }
                    )
                    try await forwarder.finish()
                    try await controller.send(.modelResponse(.init(
                        requestID: request.id,
                        message: message
                    )))
                } catch {
                    try await controller.send(.modelResponse(.init(
                        requestID: request.id,
                        message: nil,
                        failure: remoteFailure(error)
                    )))
                }
            case let .toolRequest(request):
                do {
                    let outcome = try await execute(request.call)
                    try await controller.send(.toolResponse(.init(
                        requestID: request.id,
                        outcome: outcome
                    )))
                } catch {
                    try await controller.send(.toolResponse(.init(
                        requestID: request.id,
                        outcome: nil,
                        failure: remoteFailure(error)
                    )))
                }
            case let .completionRequest(request):
                do {
                    let result = try await completionCheck()
                    try await controller.send(.completionResponse(.init(
                        requestID: request.id,
                        result: result
                    )))
                } catch {
                    try await controller.send(.completionResponse(.init(
                        requestID: request.id,
                        result: nil,
                        failure: remoteFailure(error)
                    )))
                }
            case let .contextRequest(request):
                guard let contextProvider else {
                    try await controller.send(.contextResponse(.init(
                        requestID: request.id,
                        failure: .init(detail: AgentContextError.unavailable.localizedDescription)
                    )))
                    continue
                }
                do {
                    let prepared = try await contextProvider.prepare(
                        checkpoint: request.checkpoint,
                        tools: request.tools,
                        policy: request.policy,
                        deadline: request.deadline,
                        synchronizeOnly: request.synchronizeOnly,
                        shouldPause: { Task.isCancelled },
                        record: record
                    )
                    try await controller.send(.contextResponse(.init(
                        requestID: request.id,
                        prepared: prepared
                    )))
                } catch let failure as AgentContextPreparationFailure {
                    try await controller.send(.contextResponse(.init(
                        requestID: request.id,
                        contextFailure: .init(
                            checkpoint: failure.checkpoint,
                            reason: failure.reason,
                            cancelled: failure.cancelled
                        )
                    )))
                } catch {
                    try await controller.send(.contextResponse(.init(
                        requestID: request.id,
                        failure: remoteFailure(error)
                    )))
                }
            case let .recordRequest(request):
                do {
                    try await record(request.checkpoint, request.event)
                    try await controller.send(.recordResponse(.init(requestID: request.id)))
                } catch {
                    try await controller.send(.recordResponse(.init(
                        requestID: request.id,
                        failure: remoteFailure(error)
                    )))
                }
            case let .finished(finished):
                guard finished.checkpoint.id == checkpoint.id,
                      finished.checkpoint.scope == scope else {
                    throw AgentWorkerProcessClientError.invalidResponse
                }
                let status = await controller.waitForExit()
                guard status == EXIT_SUCCESS else {
                    throw AgentWorkerProcessClientError.workerFailed(controller.stderrTail())
                }
                return finished.checkpoint
            case let .failed(failure):
                throw AgentWorkerProcessClientError.workerFailed(failure.detail)
            default:
                throw AgentWorkerProcessClientError.invalidResponse
            }
        }
    }

    private static func remoteFailure(_ error: Error) -> AgentWorkerRemoteFailure {
        .init(
            detail: error.localizedDescription,
            transient: AgentRuntimeError.isTransient(error)
        )
    }

    private static func resolveExecutable() throws -> URL {
        #if DEBUG
        if let configured = ProcessInfo.processInfo.environment["CHATOS_AGENT_WORKER_PATH"]?
            .trimmingCharacters(in: .whitespacesAndNewlines),
           !configured.isEmpty {
            return URL(fileURLWithPath: configured)
        }
        #endif
        guard let bundled = Bundle.main.url(forAuxiliaryExecutable: "ChatOSAgentWorker") else {
            throw AgentWorkerProcessClientError.unavailable
        }
        return bundled
    }

    private static func validateExecutable(_ url: URL) throws {
        var status = stat()
        guard lstat(url.path, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              status.st_uid == geteuid(),
              status.st_nlink == 1,
              access(url.path, X_OK) == 0 else {
            throw AgentWorkerProcessClientError.invalidExecutable
        }
    }
}

private actor AgentWorkerModelEventForwarder {
    let controller: AgentWorkerProcessController
    let requestID: UUID
    var failure: Error?

    init(controller: AgentWorkerProcessController, requestID: UUID) {
        self.controller = controller
        self.requestID = requestID
    }

    func forward(_ event: AgentModelStreamEvent) async {
        guard failure == nil else { return }
        do {
            try await controller.send(.modelEvent(.init(requestID: requestID, event: event)))
        } catch {
            failure = error
            controller.terminate()
        }
    }

    func finish() throws {
        if let failure { throw failure }
    }
}

private final class AgentWorkerLineReader: @unchecked Sendable {
    private let lock = NSLock()
    private var file: UnsafeMutablePointer<FILE>?

    init(descriptor: Int32) throws {
        guard let file = fdopen(dup(descriptor), "r") else {
            throw AgentWorkerProcessClientError.launchFailed
        }
        self.file = file
    }

    deinit {
        if let file { fclose(file) }
    }

    func next() throws -> Data {
        lock.lock()
        defer { lock.unlock() }
        guard let file else { throw AgentWorkerProcessClientError.invalidResponse }
        var line: UnsafeMutablePointer<CChar>?
        var capacity = 0
        defer { free(line) }
        let count = getline(&line, &capacity, file)
        guard count > 0, let line else { throw AgentWorkerProcessClientError.invalidResponse }
        let payloadCount = line[count - 1] == 0x0A ? count - 1 : count
        guard payloadCount > 0,
              payloadCount <= ChatOSAgentWorkerProtocol.maximumInlineBytes else {
            throw AgentWorkerProcessClientError.invalidResponse
        }
        return Data(bytes: line, count: payloadCount)
    }
}

private final class AgentWorkerLineWriter: @unchecked Sendable {
    private let lock = NSLock()
    private let descriptor: Int32

    init(descriptor: Int32) throws {
        let duplicated = dup(descriptor)
        guard duplicated >= 0,
              fcntl(duplicated, F_SETNOSIGPIPE, 1) == 0 else {
            if duplicated >= 0 { Darwin.close(duplicated) }
            throw AgentWorkerProcessClientError.launchFailed
        }
        self.descriptor = duplicated
    }

    deinit { if descriptor >= 0 { Darwin.close(descriptor) } }

    func write(_ data: Data) throws {
        lock.lock()
        defer { lock.unlock() }
        guard descriptor >= 0 else { throw AgentWorkerProcessClientError.invalidResponse }
        try data.withUnsafeBytes { rawBuffer in
            guard let baseAddress = rawBuffer.baseAddress else { return }
            var offset = 0
            while offset < rawBuffer.count {
                let count = Darwin.write(
                    descriptor,
                    baseAddress.advanced(by: offset),
                    rawBuffer.count - offset
                )
                if count > 0 { offset += count }
                else if count < 0, errno == EINTR { continue }
                else { throw AgentWorkerProcessClientError.invalidResponse }
            }
        }
    }
}

private final class AgentWorkerProcessController: @unchecked Sendable {
    private let process: Process
    private let reader: AgentWorkerLineReader
    private let writer: AgentWorkerLineWriter
    private let stateLock = NSLock()
    private var stderr = Data()
    private var terminationRequested = false

    var processIdentifier: Int32 { process.processIdentifier }

    init(executableURL: URL, parentProcessID: Int32) throws {
        let input = Pipe()
        let output = Pipe()
        let error = Pipe()
        let process = Process()
        process.executableURL = executableURL
        process.arguments = ["--parent-pid", String(parentProcessID)]
        process.standardInput = input
        process.standardOutput = output
        process.standardError = error
        let reader = try AgentWorkerLineReader(
            descriptor: output.fileHandleForReading.fileDescriptor
        )
        let writer = try AgentWorkerLineWriter(
            descriptor: input.fileHandleForWriting.fileDescriptor
        )
        do {
            try process.run()
        } catch {
            throw AgentWorkerProcessClientError.launchFailed
        }
        input.fileHandleForReading.closeFile()
        input.fileHandleForWriting.closeFile()
        output.fileHandleForReading.closeFile()
        output.fileHandleForWriting.closeFile()
        error.fileHandleForWriting.closeFile()
        self.process = process
        self.reader = reader
        self.writer = writer
        Task.detached(priority: .utility) { [weak self] in
            while let chunk = try? error.fileHandleForReading.read(upToCount: 4_096),
                  !chunk.isEmpty {
                self?.appendStderr(chunk)
            }
        }
    }

    deinit { terminateIfRunning() }

    func send(_ message: AgentWorkerMessage) async throws {
        let data = try AgentWorkerWireCodec.encodeLine(message)
        try await Task.detached(priority: .userInitiated) { [writer] in
            try writer.write(data)
        }.value
    }

    func receive() async throws -> AgentWorkerMessage {
        let data = try await Task.detached(priority: .userInitiated) { [reader] in
            try reader.next()
        }.value
        return try AgentWorkerWireCodec.decode(data)
    }

    func terminate() {
        stateLock.lock()
        guard process.isRunning, !terminationRequested else {
            stateLock.unlock()
            return
        }
        terminationRequested = true
        let processID = process.processIdentifier
        process.terminate()
        stateLock.unlock()
        Task.detached(priority: .utility) { [process] in
            try? await Task.sleep(for: .seconds(2))
            if process.isRunning {
                _ = Darwin.kill(processID, SIGKILL)
            }
        }
    }

    func terminateIfRunning() {
        terminate()
    }

    func waitForExit() async -> Int32 {
        await Task.detached(priority: .utility) { [process] in
            process.waitUntilExit()
            return process.terminationStatus
        }.value
    }

    func stderrTail() -> String {
        stateLock.lock()
        defer { stateLock.unlock() }
        return String(decoding: stderr.suffix(4_096), as: UTF8.self)
    }

    private func appendStderr(_ data: Data) {
        stateLock.lock()
        stderr.append(data)
        if stderr.count > 8_192 { stderr.removeFirst(stderr.count - 8_192) }
        stateLock.unlock()
    }
}
