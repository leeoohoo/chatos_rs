import ChatOSAgentRuntime
import ChatOSAgentWorkerProtocol
import ChatOSProcessRuntime
import Darwin
import Dispatch
import Foundation

public enum AgentWorkerProcessClientError: LocalizedError, Equatable {
    case unavailable
    case invalidExecutable
    case launchFailed
    case incompatibleProtocol
    case invalidResponse
    case workerFailed(String)
    case timedOut
    case resourceLimitExceeded(resource: String, observed: UInt64, limit: UInt64)

    public var errorDescription: String? {
        switch self {
        case .unavailable: "Agent 独立执行进程不可用。"
        case .invalidExecutable: "Agent 独立执行进程未通过本机文件校验。"
        case .launchFailed: "无法启动 Agent 独立执行进程。"
        case .incompatibleProtocol: "Agent 独立执行进程与客户端协议不兼容。"
        case .invalidResponse: "Agent 独立执行进程返回了无效结果。"
        case let .workerFailed(detail): "Agent 独立执行进程失败：\(detail)"
        case .timedOut: "Agent 独立执行进程超时，已被终止。"
        case let .resourceLimitExceeded(resource, observed, limit):
            "Agent 独立执行进程的\(resource)超出限制（\(observed) / \(limit)），已只终止本次任务。"
        }
    }
}

public struct AgentWorkerResourcePolicy: Sendable, Equatable {
    public var maximumPhysicalMemoryBytes: UInt64
    public var maximumCPUTimeSeconds: TimeInterval?
    public var sampleIntervalSeconds: TimeInterval

    public init(
        maximumPhysicalMemoryBytes: UInt64 = 1_610_612_736,
        maximumCPUTimeSeconds: TimeInterval? = 900,
        sampleIntervalSeconds: TimeInterval = 0.25
    ) {
        self.maximumPhysicalMemoryBytes = maximumPhysicalMemoryBytes
        self.maximumCPUTimeSeconds = maximumCPUTimeSeconds
        self.sampleIntervalSeconds = sampleIntervalSeconds
    }
}

/// Launches exactly one helper process for one Agent Run. Model, Memory, tools and SQLite remain
/// parent-owned capabilities and are invoked through the versioned pipe protocol; the worker owns
/// the orchestration loop, timeout state and checkpoint transitions for only this Run.
public struct AgentWorkerProcessClient: AgentRuntimeExecuting, Sendable {
    public typealias ProcessObserver = @Sendable (_ processID: Int32) -> Void

    public let executableURL: URL?
    public let shutdownGraceSeconds: TimeInterval
    public let resourcePolicy: AgentWorkerResourcePolicy
    private let processObserver: ProcessObserver?

    public init(
        executableURL: URL? = nil,
        shutdownGraceSeconds: TimeInterval = 30,
        resourcePolicy: AgentWorkerResourcePolicy = .init(),
        processObserver: ProcessObserver? = nil
    ) {
        self.executableURL = executableURL
        self.shutdownGraceSeconds = shutdownGraceSeconds
        self.resourcePolicy = resourcePolicy
        self.processObserver = processObserver
    }

    public func checkExecutionAdmission() async throws {
        let resolvedExecutable = try executableURL ?? Self.resolveExecutable()
        try Self.validateExecutable(resolvedExecutable)
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
              (1...120).contains(shutdownGraceSeconds),
              resourcePolicy.maximumPhysicalMemoryBytes > 0,
              resourcePolicy.sampleIntervalSeconds.isFinite,
              (0.01...10).contains(resourcePolicy.sampleIntervalSeconds),
              resourcePolicy.maximumCPUTimeSeconds.map({
                  $0.isFinite && (1...86_400).contains($0)
              }) ?? true else {
            throw AgentWorkerProcessClientError.invalidResponse
        }
        let resolvedExecutable = try executableURL ?? Self.resolveExecutable()
        try Self.validateExecutable(resolvedExecutable)
        let modelTransportConfiguration = model.workerTransportConfiguration
        if let modelTransportConfiguration {
            guard modelTransportConfiguration.usesServerSideCompaction
                    == model.usesServerSideCompaction else {
                throw AgentWorkerProcessClientError.invalidResponse
            }
            try await model.validateWorkerTransportAuthorization()
        }
        do {
            let controller = try AgentWorkerProcessController(
                executableURL: resolvedExecutable,
                parentProcessID: getpid()
            )
            processObserver?(controller.processIdentifier)
            let timeout = Double(policy.runTimeoutSeconds) + shutdownGraceSeconds
            let result = try await withTaskCancellationHandler {
                do {
                    return try await withAgentTimeout(seconds: timeout) {
                        try await Self.runSupervisedProtocol(
                            controller: controller,
                            resourcePolicy: resourcePolicy,
                            checkpoint: checkpoint,
                            scope: scope,
                            policy: policy,
                            model: model,
                            tools: tools,
                            execute: execute,
                            completionCheck: completionCheck,
                            contextProvider: contextProvider,
                            modelTransportConfiguration: modelTransportConfiguration,
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
            return result
        }
    }

    private enum SupervisedResult: Sendable {
        case checkpoint(AgentRunCheckpoint)
        case monitorStopped
    }

    private static func runSupervisedProtocol(
        controller: AgentWorkerProcessController,
        resourcePolicy: AgentWorkerResourcePolicy,
        checkpoint: AgentRunCheckpoint,
        scope: String,
        policy: AgentRunPolicy,
        model: any AgentModelClient,
        tools: [AgentToolDefinition],
        execute: @escaping AgentRuntime.Executor,
        completionCheck: @escaping AgentRuntime.CompletionCheck,
        contextProvider: (any AgentContextProviding)?,
        modelTransportConfiguration: AgentWorkerModelTransportConfiguration?,
        record: @escaping AgentRuntime.Recorder
    ) async throws -> AgentRunCheckpoint {
        do {
            return try await withThrowingTaskGroup(of: SupervisedResult.self) { group in
                group.addTask {
                    .checkpoint(try await runProtocol(
                        controller: controller,
                        checkpoint: checkpoint,
                        scope: scope,
                        policy: policy,
                        model: model,
                        tools: tools,
                        execute: execute,
                        completionCheck: completionCheck,
                        contextProvider: contextProvider,
                        modelTransportConfiguration: modelTransportConfiguration,
                        record: record
                    ))
                }
                group.addTask {
                    try await controller.monitorResources(policy: resourcePolicy)
                    return .monitorStopped
                }
                while let result = try await group.next() {
                    switch result {
                    case let .checkpoint(checkpoint):
                        group.cancelAll()
                        return checkpoint
                    case .monitorStopped:
                        continue
                    }
                }
                throw AgentWorkerProcessClientError.invalidResponse
            }
        } catch {
            if let violation = controller.resourceViolation() { throw violation }
            throw error
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
        modelTransportConfiguration: AgentWorkerModelTransportConfiguration?,
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
            modelUsesServerSideCompaction: model.usesServerSideCompaction,
            modelTransportConfiguration: modelTransportConfiguration
        )))

        while true {
            try Task.checkCancellation()
            switch try await controller.receive() {
            case let .modelRequest(request):
                guard modelTransportConfiguration == nil else {
                    throw AgentWorkerProcessClientError.invalidResponse
                }
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
            case let .modelAuthorizationRequest(request):
                guard modelTransportConfiguration != nil else {
                    throw AgentWorkerProcessClientError.invalidResponse
                }
                do {
                    try await model.validateWorkerTransportAuthorization()
                    try await controller.send(.modelAuthorizationResponse(.init(
                        requestID: request.id
                    )))
                } catch {
                    try await controller.send(.modelAuthorizationResponse(.init(
                        requestID: request.id,
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
                // `finished` is the Worker's committed protocol result. Waiting for Foundation's
                // separate `Process.waitUntilExit()` notification after this point can miss a
                // very fast `_exit` under repeated/concurrent launches and hold an already
                // completed Run until its outer timeout. The controller's deinit still
                // terminates a malformed Worker that sends `finished` but remains alive.
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
    private let descriptor: Int32
    private var buffered = Data()

    init(descriptor: Int32) throws {
        let duplicated = dup(descriptor)
        guard duplicated >= 0 else {
            throw AgentWorkerProcessClientError.launchFailed
        }
        let flags = fcntl(duplicated, F_GETFL)
        guard flags >= 0,
              fcntl(duplicated, F_SETFL, flags | O_NONBLOCK) == 0 else {
            Darwin.close(duplicated)
            throw AgentWorkerProcessClientError.launchFailed
        }
        self.descriptor = duplicated
    }

    deinit { if descriptor >= 0 { Darwin.close(descriptor) } }

    func next(processIsAlive: () -> Bool) throws -> Data {
        lock.lock()
        defer { lock.unlock() }
        while true {
            if let newline = buffered.firstIndex(of: 0x0A) {
                let payload = Data(buffered[..<newline])
                buffered.removeSubrange(...newline)
                guard !payload.isEmpty,
                      payload.count <= ChatOSAgentWorkerProtocol.maximumInlineBytes else {
                    throw AgentWorkerProcessClientError.invalidResponse
                }
                return payload
            }
            guard buffered.count <= ChatOSAgentWorkerProtocol.maximumInlineBytes else {
                throw AgentWorkerProcessClientError.invalidResponse
            }

            var descriptorState = pollfd(
                fd: descriptor,
                events: Int16(POLLIN | POLLHUP | POLLERR),
                revents: 0
            )
            let pollResult = Darwin.poll(&descriptorState, 1, 100)
            if pollResult == 0 {
                guard processIsAlive() else {
                    throw AgentWorkerProcessClientError.invalidResponse
                }
                continue
            }
            if pollResult < 0 {
                if errno == EINTR { continue }
                throw AgentWorkerProcessClientError.invalidResponse
            }
            guard descriptorState.revents & Int16(POLLNVAL) == 0 else {
                throw AgentWorkerProcessClientError.invalidResponse
            }

            var chunk = [UInt8](repeating: 0, count: 64 * 1_024)
            let count = chunk.withUnsafeMutableBytes { bytes in
                Darwin.read(descriptor, bytes.baseAddress, bytes.count)
            }
            if count > 0 {
                buffered.append(contentsOf: chunk.prefix(count))
                continue
            }
            if count == 0 {
                guard !buffered.isEmpty,
                      buffered.count <= ChatOSAgentWorkerProtocol.maximumInlineBytes else {
                    throw AgentWorkerProcessClientError.invalidResponse
                }
                defer { buffered.removeAll(keepingCapacity: false) }
                return buffered
            }
            if errno == EINTR || errno == EAGAIN { continue }
            throw AgentWorkerProcessClientError.invalidResponse
        }
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
    /// Pipe polling/reads and writes are blocking syscalls. Running them in `Task.detached`
    /// occupies Swift cooperative-executor threads for the lifetime of a Run and can starve
    /// unrelated conversations when many Workers are alive. GCD owns these blocking lanes and
    /// may compensate with kernel threads without consuming Swift executor capacity.
    private static let protocolIOQueue = DispatchQueue(
        label: "com.chatos.agent-worker.protocol-io",
        qos: .userInitiated,
        attributes: .concurrent
    )
    private let process: Process
    private let reader: AgentWorkerLineReader
    private let writer: AgentWorkerLineWriter
    private let errorReadHandle: FileHandle
    private let startSeconds: UInt64
    private let startMicroseconds: UInt64
    private let stateLock = NSLock()
    private var stderr = Data()
    private var terminationRequested = false
    private var recordedResourceViolation: AgentWorkerProcessClientError?

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
        var startSeconds: UInt64 = 0
        var startMicroseconds: UInt64 = 0
        guard chatos_process_start_time(
            process.processIdentifier,
            &startSeconds,
            &startMicroseconds
        ) == 0 else {
            process.terminate()
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
        self.errorReadHandle = error.fileHandleForReading
        self.startSeconds = startSeconds
        self.startMicroseconds = startMicroseconds
        self.errorReadHandle.readabilityHandler = { [weak self] handle in
            let chunk = handle.availableData
            guard !chunk.isEmpty else {
                handle.readabilityHandler = nil
                return
            }
            self?.appendStderr(chunk)
        }
    }

    deinit {
        errorReadHandle.readabilityHandler = nil
        try? errorReadHandle.close()
        terminateIfRunning()
    }

    func send(_ message: AgentWorkerMessage) async throws {
        let data = try AgentWorkerWireCodec.encodeLine(message)
        try await withCheckedThrowingContinuation { continuation in
            Self.protocolIOQueue.async { [writer] in
                do {
                    try writer.write(data)
                    continuation.resume(returning: ())
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    func receive() async throws -> AgentWorkerMessage {
        let processID = process.processIdentifier
        let expectedStartSeconds = startSeconds
        let expectedStartMicroseconds = startMicroseconds
        let data: Data = try await withCheckedThrowingContinuation { continuation in
            Self.protocolIOQueue.async { [reader] in
                do {
                    continuation.resume(returning: try reader.next {
                        chatos_process_matches_start_time(
                            processID,
                            expectedStartSeconds,
                            expectedStartMicroseconds
                        ) == 1
                    })
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
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

    func monitorResources(policy: AgentWorkerResourcePolicy) async throws {
        let maximumCPUNanoseconds = policy.maximumCPUTimeSeconds.map { seconds -> UInt64 in
            let nanoseconds = seconds * 1_000_000_000
            return nanoseconds >= Double(UInt64.max) ? UInt64.max : UInt64(nanoseconds)
        }
        while !Task.isCancelled {
            try await Task.sleep(for: .seconds(policy.sampleIntervalSeconds))
            try Task.checkCancellation()
            guard process.isRunning else { return }
            var physicalFootprint: UInt64 = 0
            var cpuTime: UInt64 = 0
            let result = chatos_process_resource_usage(
                process.processIdentifier,
                startSeconds,
                startMicroseconds,
                &physicalFootprint,
                &cpuTime
            )
            if result == ESRCH { return }
            guard result == 0 else { continue }
            if physicalFootprint > policy.maximumPhysicalMemoryBytes {
                let violation = AgentWorkerProcessClientError.resourceLimitExceeded(
                    resource: "物理内存",
                    observed: physicalFootprint,
                    limit: policy.maximumPhysicalMemoryBytes
                )
                recordAndTerminate(violation)
                throw violation
            }
            if let maximumCPUNanoseconds, cpuTime > maximumCPUNanoseconds {
                let violation = AgentWorkerProcessClientError.resourceLimitExceeded(
                    resource: "累计 CPU 时间（纳秒）",
                    observed: cpuTime,
                    limit: maximumCPUNanoseconds
                )
                recordAndTerminate(violation)
                throw violation
            }
        }
    }

    func resourceViolation() -> AgentWorkerProcessClientError? {
        stateLock.lock()
        defer { stateLock.unlock() }
        return recordedResourceViolation
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

    private func recordAndTerminate(_ violation: AgentWorkerProcessClientError) {
        stateLock.lock()
        if recordedResourceViolation == nil { recordedResourceViolation = violation }
        stateLock.unlock()
        terminate()
    }
}
