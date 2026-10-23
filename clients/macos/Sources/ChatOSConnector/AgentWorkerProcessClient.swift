import ChatOSAgentRuntime
import ChatOSAgentWorkerProtocol
import ChatOSProcessRuntime
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
    case temporarilyUnavailable(retryAfterSeconds: Int)
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
        case let .temporarilyUnavailable(retryAfterSeconds):
            "Agent 独立执行进程连续异常，已暂停接收新任务，请在约 \(retryAfterSeconds) 秒后重试。"
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

struct AgentWorkerExecutableIdentity: Hashable, Sendable {
    let path: String
    let device: UInt64
    let inode: UInt64
    let size: UInt64
    let modificationSeconds: Int64
    let modificationNanoseconds: Int64
}

actor AgentWorkerFailureCircuitBreaker {
    struct Configuration: Sendable {
        var failureWindowSeconds: TimeInterval = 300
        var baseBackoffSeconds: TimeInterval = 1
        var maximumBackoffSeconds: TimeInterval = 30
        var quarantineFailureCount = 5
        var quarantineSeconds: TimeInterval = 30
    }

    struct Snapshot: Sendable, Equatable {
        let abnormalFailureCount: Int
        let unavailableUntil: TimeInterval?
    }

    private struct State {
        var failureDates: [TimeInterval] = []
        var unavailableUntil: TimeInterval?
    }

    private let configuration: Configuration
    private var states: [AgentWorkerExecutableIdentity: State] = [:]

    init(configuration: Configuration = .init()) {
        self.configuration = configuration
    }

    func checkAdmission(for identity: AgentWorkerExecutableIdentity) throws {
        let now = Date().timeIntervalSince1970
        var state = pruned(states[identity] ?? .init(), now: now)
        let backoffUntil: TimeInterval? = state.failureDates.last.map { lastFailure in
            let exponent = min(max(0, state.failureDates.count - 1), 20)
            let delay = min(
                configuration.maximumBackoffSeconds,
                configuration.baseBackoffSeconds * pow(2, Double(exponent))
            )
            return lastFailure + delay
        }
        let blockedUntil = max(state.unavailableUntil ?? 0, backoffUntil ?? 0)
        if blockedUntil > now {
            states[identity] = state
            throw AgentWorkerProcessClientError.temporarilyUnavailable(
                retryAfterSeconds: max(1, Int(ceil(blockedUntil - now)))
            )
        }
        state.unavailableUntil = nil
        states[identity] = state
    }

    func recordSuccess(for identity: AgentWorkerExecutableIdentity) {
        states.removeValue(forKey: identity)
    }

    func recordAbnormalFailure(for identity: AgentWorkerExecutableIdentity) {
        let now = Date().timeIntervalSince1970
        var state = pruned(states[identity] ?? .init(), now: now)
        state.failureDates.append(now)
        if state.failureDates.count >= configuration.quarantineFailureCount {
            state.unavailableUntil = now + configuration.quarantineSeconds
        }
        states[identity] = state
    }

    func snapshot(for identity: AgentWorkerExecutableIdentity) -> Snapshot {
        let state = pruned(states[identity] ?? .init(), now: Date().timeIntervalSince1970)
        return .init(
            abnormalFailureCount: state.failureDates.count,
            unavailableUntil: state.unavailableUntil
        )
    }

    func totalAbnormalFailureCount() -> Int {
        let now = Date().timeIntervalSince1970
        return states.values.reduce(into: 0) { total, value in
            total += pruned(value, now: now).failureDates.count
        }
    }

    private func pruned(_ value: State, now: TimeInterval) -> State {
        var state = value
        state.failureDates.removeAll { now - $0 > configuration.failureWindowSeconds }
        if let unavailableUntil = state.unavailableUntil, unavailableUntil <= now {
            state.unavailableUntil = nil
        }
        return state
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
    private let circuitBreaker: AgentWorkerFailureCircuitBreaker
    private static let sharedCircuitBreaker = AgentWorkerFailureCircuitBreaker()

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
        self.circuitBreaker = Self.sharedCircuitBreaker
    }

    init(
        executableURL: URL?,
        shutdownGraceSeconds: TimeInterval,
        resourcePolicy: AgentWorkerResourcePolicy,
        processObserver: ProcessObserver? = nil,
        circuitBreaker: AgentWorkerFailureCircuitBreaker
    ) {
        self.executableURL = executableURL
        self.shutdownGraceSeconds = shutdownGraceSeconds
        self.resourcePolicy = resourcePolicy
        self.processObserver = processObserver
        self.circuitBreaker = circuitBreaker
    }

    public func checkExecutionAdmission() async throws {
        let resolvedExecutable = try executableURL ?? Self.resolveExecutable()
        let identity = try Self.validateExecutable(resolvedExecutable)
        try await circuitBreaker.checkAdmission(for: identity)
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
        let executableIdentity = try Self.validateExecutable(resolvedExecutable)
        try await circuitBreaker.checkAdmission(for: executableIdentity)
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
            await circuitBreaker.recordSuccess(for: executableIdentity)
            return result
        } catch {
            if Self.isAbnormalInfrastructureFailure(error) {
                await circuitBreaker.recordAbnormalFailure(for: executableIdentity)
            }
            throw error
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

    private static func isAbnormalInfrastructureFailure(_ error: Error) -> Bool {
        guard let error = error as? AgentWorkerProcessClientError else { return false }
        switch error {
        case .launchFailed, .incompatibleProtocol, .invalidResponse, .resourceLimitExceeded:
            return true
        case .unavailable, .invalidExecutable, .workerFailed, .timedOut,
             .temporarilyUnavailable:
            return false
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

    private static func validateExecutable(_ url: URL) throws -> AgentWorkerExecutableIdentity {
        var status = stat()
        guard lstat(url.path, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              status.st_uid == geteuid(),
              status.st_nlink == 1,
              access(url.path, X_OK) == 0 else {
            throw AgentWorkerProcessClientError.invalidExecutable
        }
        return .init(
            path: url.standardizedFileURL.path,
            device: UInt64(status.st_dev),
            inode: UInt64(status.st_ino),
            size: UInt64(status.st_size),
            modificationSeconds: Int64(status.st_mtimespec.tv_sec),
            modificationNanoseconds: Int64(status.st_mtimespec.tv_nsec)
        )
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
        self.startSeconds = startSeconds
        self.startMicroseconds = startMicroseconds
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

    private func recordAndTerminate(_ violation: AgentWorkerProcessClientError) {
        stateLock.lock()
        if recordedResourceViolation == nil { recordedResourceViolation = violation }
        stateLock.unlock()
        terminate()
    }
}
