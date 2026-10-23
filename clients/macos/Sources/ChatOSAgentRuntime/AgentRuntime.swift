import CryptoKit
import Foundation

public struct AgentRuntime: Sendable {
    private static let toolResultReadToolName = "tool_result_read"
    private static let inlineToolResultMaximumCharacters = 160_000
    private static let inlineToolResultBatchMaximumCharacters = 800_000
    private static let toolResultReadMaximumCharacters = 8_000

    public typealias RetrySleeper = @Sendable (Duration) async throws -> Void
    private let retrySleeper: RetrySleeper

    public init(retrySleeper: @escaping RetrySleeper = { duration in
        try await Task.sleep(for: duration)
    }) {
        self.retrySleeper = retrySleeper
    }
    public typealias Executor = @Sendable (AgentToolCall) async throws -> AgentToolOutcome
    public typealias CompletionCheck = @Sendable () async throws -> String?
    public typealias Recorder = @Sendable (AgentRunCheckpoint, AgentRunEvent) async throws -> Void

    public func run(
        checkpoint initial: AgentRunCheckpoint, scope: String, policy: AgentRunPolicy,
        model: any AgentModelClient, tools: [AgentToolDefinition],
        execute: @escaping Executor,
        completionCheck: @escaping CompletionCheck = { nil },
        contextProvider: (any AgentContextProviding)? = nil,
        shouldPause: @escaping @Sendable () async -> Bool = { false },
        onModelStreamEvent: @escaping @Sendable (AgentModelStreamEvent) async -> Void = { _ in },
        record: @escaping Recorder = { _, _ in }
    ) async throws -> AgentRunCheckpoint {
        try policy.validate()
        guard initial.scope == scope else { throw AgentRuntimeError.scopeMismatch }
        guard Set(tools.map(\.name)).count == tools.count,
              !tools.contains(where: { $0.name == Self.toolResultReadToolName }) else {
            throw AgentRuntimeError.invalidResponse
        }
        let modelTools = tools + [Self.toolResultReadDefinition]
        if let inFlight = initial.inFlightCallID, !initial.pendingCalls.contains(where: { $0.id == inFlight }) {
            throw AgentRuntimeError.invalidResponse
        }
        var state = initial
        if state.status == .completed { return state }
        let started = Date()
        let elapsedBefore = state.elapsedSeconds
        let registry = Dictionary(uniqueKeysWithValues: modelTools.map { ($0.name, $0) })
        let contextPolicy = policy.context ?? AgentContextPolicy()
        // Memory Engine is composed once at the start of this run/resume. New
        // messages are appended locally for the rest of the model/tool loop so
        // official OpenAI Responses can keep one stable continuation and let
        // server-side compaction own in-run context management.
        var activeModelMessages: [AgentMessage]?
        var activeCheckpointMessageCount = state.messages.count
        let deadline = started.addingTimeInterval(Double(policy.runTimeoutSeconds) - elapsedBefore)
        func snapshot(_ value: AgentRunCheckpoint) -> AgentRunCheckpoint {
            var copy = value; copy.elapsedSeconds = elapsedBefore + Date().timeIntervalSince(started); return copy
        }
        func emit(_ kind: String, _ detail: String) async throws {
            try await record(snapshot(state), .init(kind: kind, detail: detail, modelCalls: state.modelCalls))
        }
        func remainingTime() -> Double { Double(policy.runTimeoutSeconds) - elapsedBefore - Date().timeIntervalSince(started) }
        if let id = state.inFlightCallID, let call = state.pendingCalls.first(where: { $0.id == id }),
           registry[call.name]?.effect != .readOnly {
            state.status = .needsReview
            state.stopReason = "上次副作用工具执行结果不明，需要核实原任务，不能自动重放。"
            try await emit("needs_review", "上次副作用工具执行结果不明，需要核实原任务，不能自动重放。")
            return snapshot(state)
        }
        // A paused checkpoint has already consumed its previous no-progress window. Invoking
        // `run` again is the resume boundary, so grant a fresh progress window while preserving
        // the elapsed run deadline, model/tool history, receipts, and durable side-effect guards.
        if state.status == .paused {
            state.noProgressRounds = 0
        }
        state.status = .running
        state.stopReason = nil
        try await emit("started", "Agent 运行开始 / 恢复")
        do {
            while true {
                try Task.checkCancellation()
                // Flush every newly appended user/assistant/tool message before
                // handling another tool or taking any normal early-exit path.
                // This keeps Memory Engine as the complete audit transcript even
                // though it is composed only once per run/resume boundary.
                if let contextProvider, let memory = state.memory,
                   memory.syncedMessageCount < state.messages.count {
                    let synced = try await contextProvider.prepare(
                        checkpoint: snapshot(state), tools: modelTools, policy: contextPolicy,
                        deadline: deadline, synchronizeOnly: true,
                        shouldPause: { false }, record: record
                    )
                    state = synced.checkpoint
                    if activeModelMessages != nil {
                        guard activeCheckpointMessageCount <= state.messages.count else {
                            throw AgentContextError.invalidHistory
                        }
                        activeModelMessages!.append(
                            contentsOf: state.messages[activeCheckpointMessageCount...]
                        )
                        activeCheckpointMessageCount = state.messages.count
                    }
                } else if state.memory != nil, contextProvider == nil {
                    throw AgentContextError.unavailable
                }
                if await shouldPause() {
                    state.status = .paused; try await emit("paused", "已保存检查点，后续步骤暂停")
                    return snapshot(state)
                }
                guard remainingTime() > 0 else { throw AgentRuntimeError.timeout }
                if state.completionResult == nil, state.pendingCalls.isEmpty,
                   let completion = try await completionCheck() {
                    state.completionResult = completion
                    try await emit("completion_detected", "业务数据已通过本地完整性校验，无需再次调用模型")
                }
                if let completion = state.completionResult {
                    if let contextProvider {
                        let synced = try await contextProvider.prepare(checkpoint: snapshot(state), tools: modelTools, policy: contextPolicy,
                            deadline: deadline, synchronizeOnly: true, shouldPause: shouldPause, record: record)
                        state = synced.checkpoint
                    } else if state.memory != nil { throw AgentContextError.unavailable }
                    state.status = .completed; state.result = completion
                    try await emit("completed", "业务完成条件已确认")
                    return snapshot(state)
                }
                if !state.pendingCalls.isEmpty {
                    let call = state.pendingCalls[0]
                    let definition = registry[call.name]
                    var outcome: AgentToolOutcome
                    if let saved = state.receipts[call.id] { outcome = saved }
                    else if let definition {
                        do {
                            try AgentSchemaValidator.validate(arguments: call.arguments, schema: definition.schema)
                        } catch {
                            outcome = .failure(error.localizedDescription)
                            state.receipts[call.id] = outcome
                            state.messages.append(.init(role: .tool, content: outcome.content, toolCallID: call.id))
                            state.pendingCalls.removeFirst()
                            try await emit("tool_rejected", "\(call.name)：\(outcome.content)")
                            continue
                        }
                        state.inFlightCallID = call.id
                        try await emit("tool_started", call.name) // Durable before execution.
                        do {
                            if call.name == Self.toolResultReadToolName {
                                outcome = Self.readToolResult(call: call, receipts: state.receipts)
                            } else {
                                outcome = try await withAgentTimeout(seconds: remainingTime()) { try await execute(call) }
                            }
                        } catch {
                            if error is CancellationError { throw error }
                            let interruption = if let runtimeError = error as? AgentRuntimeError,
                                                  case .timeout = runtimeError {
                                "工具在 Agent 剩余运行时限内没有返回（本次 Agent 总时限为 \(policy.runTimeoutSeconds) 秒）。工具结果可能不完整；请检查副作用后再重试。"
                            } else {
                                error.localizedDescription
                            }
                            if definition.effect == .billable || definition.effect == .write {
                                state.status = .needsReview
                                state.stopReason = "\(call.name) 执行中断：\(interruption)"
                                try await emit("needs_review", state.stopReason ?? interruption)
                                return snapshot(state)
                            }
                            outcome = .failure(interruption)
                        }
                    } else { outcome = .failure("工具不可用：\(call.name)。请选择本次提供的工具。") }

                    // Keep the complete result in the durable checkpoint receipt. Only the model
                    // projection is replaced with a reference when the per-result or current
                    // tool-batch budget is exceeded; tool_result_read pages the preserved receipt.
                    let persistedOutcome = outcome
                    let currentBatchCharacters = state.messages.reversed()
                        .prefix(while: { $0.role == .tool })
                        .reduce(into: 0) { total, message in
                            total = total.saturatingAdding(message.content.unicodeScalars.count)
                        }
                    outcome = Self.modelVisibleOutcome(
                        outcome,
                        call: call,
                        currentBatchCharacters: currentBatchCharacters
                    )
                    let signature = fingerprint(call)
                    if state.observations[signature] == outcome.content { outcome.madeProgress = false }
                    state.observations[signature] = outcome.content
                    state.receipts[call.id] = persistedOutcome
                    state.callFingerprints[call.id] = signature
                    state.inFlightCallID = nil
                    state.messages.append(.init(role: .tool, content: outcome.content, toolCallID: call.id))
                    state.pendingCalls.removeFirst()
                    if outcome.madeProgress && !outcome.isError { state.noProgressRounds = 0 }
                    if definition?.effect == .terminal && !persistedOutcome.isError {
                        state.completionResult = persistedOutcome.content
                    }
                    try await emit(outcome.isError ? "tool_rejected" : "tool_completed", "\(call.name)：\(outcome.content.prefix(400))")
                    continue
                }
                if state.modelCalls >= policy.maximumModelCalls {
                    state.stopReason = "已达到设置中的模型调用上限 \(policy.maximumModelCalls)，未完成的操作需要人工确认。"
                    state.status = .limitReached; try await emit("limit_reached", "已达到设置中的模型调用上限 \(policy.maximumModelCalls)，未宣称业务完成")
                    return snapshot(state)
                }
                if state.noProgressRounds >= policy.maximumNoProgressRounds {
                    state.stopReason = "连续无进展，已暂停。请检查工具错误或调整设置后继续。"
                    state.status = .paused; try await emit("no_progress", "连续无进展，已暂停。请检查工具错误或调整设置后继续。")
                    return snapshot(state)
                }
                var messages: [AgentMessage]
                if let contextProvider {
                    if activeModelMessages == nil {
                        let prepared = try await contextProvider.prepare(
                            checkpoint: snapshot(state), tools: modelTools, policy: contextPolicy,
                            deadline: deadline, synchronizeOnly: false,
                            shouldPause: shouldPause, record: record
                        )
                        state = prepared.checkpoint
                        activeModelMessages = prepared.messages
                        activeCheckpointMessageCount = state.messages.count
                    }
                    messages = activeModelMessages!
                } else {
                    guard state.memory == nil else { throw AgentContextError.unavailable }
                    messages = state.messages
                }
                var response: AgentMessage?
                for attempt in 0...policy.maximumRequestRetries {
                    try Task.checkCancellation()
                    if await shouldPause() { throw CancellationError() }
                    guard state.modelCalls < policy.maximumModelCalls else { break }
                    state.modelCalls += 1
                    try await emit("model_request", "模型调用 \(state.modelCalls) / \(policy.maximumModelCalls)")
                    let requestMessages = messages
                    let inactivityTimeout = min(Double(policy.requestTimeoutSeconds), remainingTime())
                    let telemetry = AgentModelAttemptTelemetry()
                    do {
                        response = try await withAgentTimeout(seconds: remainingTime()) {
                            try await withAgentInactivityTimeout(seconds: inactivityTimeout) { markActivity in
                                try await model.stream(
                                    messages: requestMessages,
                                    tools: modelTools,
                                    timeout: inactivityTimeout,
                                    onEvent: { event in
                                        telemetry.record(event)
                                        markActivity()
                                        await onModelStreamEvent(event)
                                    }
                                )
                            }
                        }
                        try await emit("model_stream_completed", telemetry.detail)
                        break
                    } catch {
                        let telemetryDetail = telemetry.detail
                        guard attempt < policy.maximumRequestRetries, AgentRuntimeError.isTransient(error) else {
                            try await emit(
                                "model_request_failed",
                                "模型请求失败（\(telemetryDetail)）：\(error.localizedDescription)"
                            )
                            throw error
                        }
                        let retryNumber = attempt + 1
                        let delaySeconds = Self.retryDelaySeconds(forRetry: retryNumber)
                        guard remainingTime() > Double(delaySeconds) else { throw AgentRuntimeError.timeout }
                        try await emit(
                            "model_retry",
                            "暂时性模型请求失败（\(telemetryDetail)）；第 \(retryNumber) / \(policy.maximumRequestRetries) 次重试将在 \(delaySeconds) 秒后开始"
                        )
                        try await retrySleeper(.seconds(delaySeconds))
                    }
                }
                guard let response else { continue }
                guard response.role == .assistant, response.toolCalls.count <= 32,
                      Set(response.toolCalls.map(\.id)).count == response.toolCalls.count,
                      response.toolCalls.allSatisfy({ !$0.id.isEmpty && state.receipts[$0.id] == nil }) else { throw AgentRuntimeError.invalidResponse }
                state.noProgressRounds += 1
                var accumulatedUsage = state.usage ?? AgentUsage()
                accumulatedUsage.add(response.usage)
                state.usage = accumulatedUsage
                state.messages.append(response)
                if response.toolCalls.isEmpty {
                    state.messages.append(.init(role: .user, content: "请根据已保存的业务状态继续调用工具；只有调用结束工具并通过业务校验才能结束。"))
                } else if response.toolCalls.count > 1 && response.toolCalls.contains(where: { registry[$0.name]?.effect == .terminal }) {
                    for call in response.toolCalls {
                        let rejection = AgentToolOutcome.failure("结束工具必须单独调用；本批次没有执行任何工具。")
                        state.receipts[call.id] = rejection
                        state.messages.append(.init(role: .tool, content: rejection.content, toolCallID: call.id))
                    }
                } else { state.pendingCalls = response.toolCalls }
                try await emit("model_response", "收到 \(response.toolCalls.count) 个工具调用")
            }
        } catch {
            if let failure = error as? AgentContextPreparationFailure {
                state = failure.checkpoint; state.status = .paused
                state.stopReason = failure.reason
                try await emit("context_paused", failure.reason)
            } else {
                state.status = error is CancellationError || error is AgentContextError ? .paused : .failed
                state.stopReason = error.localizedDescription
                try await emit("stopped", error.localizedDescription)
            }
            return snapshot(state)
        }
    }

    private func fingerprint(_ call: AgentToolCall) -> String {
        if let data = call.arguments.data(using: .utf8), let value = try? JSONSerialization.jsonObject(with: data),
           let normalized = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]) {
            return call.name + ":" + String(decoding: normalized, as: UTF8.self)
        }
        return call.name + ":" + call.arguments
    }

    static func retryDelaySeconds(forRetry retryNumber: Int) -> Int {
        guard retryNumber > 0 else { return 1 }
        return min(16, 1 << min(retryNumber - 1, 4))
    }

    private static let toolResultReadDefinition = AgentToolDefinition(
        name: toolResultReadToolName,
        description: "分页读取当前 Agent Run 中已完整保存的超大工具结果。按照 next_offset 连续读取并校验 sha256。",
        schema: Data(#"{"type":"object","properties":{"source_call_id":{"type":"string","minLength":1},"offset":{"type":"integer","minimum":0,"default":0},"limit":{"type":"integer","minimum":1,"maximum":8000,"default":8000}},"required":["source_call_id"],"additionalProperties":false}"#.utf8),
        effect: .readOnly
    )

    private struct ToolResultReadArguments: Decodable {
        let sourceCallID: String
        let offset: Int?
        let limit: Int?

        enum CodingKeys: String, CodingKey {
            case sourceCallID = "source_call_id"
            case offset
            case limit
        }
    }

    private static func modelVisibleOutcome(
        _ outcome: AgentToolOutcome,
        call: AgentToolCall,
        currentBatchCharacters: Int
    ) -> AgentToolOutcome {
        let characters = outcome.content.unicodeScalars.count
        guard characters > inlineToolResultMaximumCharacters
                || currentBatchCharacters.saturatingAdding(characters)
                    > inlineToolResultBatchMaximumCharacters else {
            return outcome
        }
        let descriptor: [String: Any] = [
            "type": "tool_result_reference",
            "source_call_id": call.id,
            "source_tool_name": call.name,
            "total_chars": characters,
            "sha256": sha256(outcome.content),
            "read_tool": toolResultReadToolName,
            "max_page_chars": toolResultReadMaximumCharacters,
            "complete": false,
            "guidance": "完整结果已保存在当前 Agent Run。请从 offset=0 开始调用 tool_result_read，按 next_offset 顺序拼接 content，直到 complete=true，并校验 sha256。源结果没有被静默截断。",
        ]
        return .init(
            canonicalJSONString(descriptor),
            madeProgress: outcome.madeProgress,
            isError: outcome.isError
        )
    }

    private static func readToolResult(
        call: AgentToolCall,
        receipts: [String: AgentToolOutcome]
    ) -> AgentToolOutcome {
        guard let data = call.arguments.data(using: .utf8),
              let arguments = try? JSONDecoder().decode(ToolResultReadArguments.self, from: data) else {
            return .failure("tool_result_read 参数无效。")
        }
        guard arguments.sourceCallID != call.id else {
            return .failure("tool_result_read 不能读取自身尚未完成的结果。")
        }
        guard let source = receipts[arguments.sourceCallID] else {
            return .failure("当前 Agent Run 中不存在这个 source_call_id。")
        }
        let offset = arguments.offset ?? 0
        let limit = arguments.limit ?? toolResultReadMaximumCharacters
        let scalars = source.content.unicodeScalars
        let totalCharacters = scalars.count
        guard offset >= 0, offset <= totalCharacters,
              limit > 0, limit <= toolResultReadMaximumCharacters else {
            return .failure("tool_result_read 的 offset 或 limit 超出允许范围。")
        }
        let endOffset = min(totalCharacters, offset.saturatingAdding(limit))
        let start = scalars.index(scalars.startIndex, offsetBy: offset)
        let end = scalars.index(start, offsetBy: endOffset - offset)
        let complete = endOffset == totalCharacters
        let page: [String: Any] = [
            "source_call_id": arguments.sourceCallID,
            "content": String(scalars[start..<end]),
            "offset": offset,
            "returned_chars": endOffset - offset,
            "next_offset": complete ? NSNull() : endOffset,
            "total_chars": totalCharacters,
            "sha256": sha256(source.content),
            "complete": complete,
        ]
        return .init(canonicalJSONString(page))
    }

    private static func sha256(_ content: String) -> String {
        SHA256.hash(data: Data(content.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
    }

    private static func canonicalJSONString(_ value: [String: Any]) -> String {
        guard JSONSerialization.isValidJSONObject(value),
              let data = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]) else {
            return #"{"error":"tool_result_reference_encoding_failed"}"#
        }
        return String(decoding: data, as: UTF8.self)
    }
}

private extension Int {
    func saturatingAdding(_ other: Int) -> Int {
        let (result, overflow) = addingReportingOverflow(other)
        return overflow ? .max : result
    }
}

/// Applies an inactivity timeout which is renewed whenever the stream reports transport
/// activity. The enclosing run deadline remains a separate absolute timeout.
func withAgentInactivityTimeout<T: Sendable>(
    seconds: Double,
    operation: @escaping @Sendable (@escaping @Sendable () -> Void) async throws -> T
) async throws -> T {
    guard seconds > 0 else { throw AgentRuntimeError.timeout }
    let race = AgentInactivityTimeoutRace<T>(seconds: seconds)
    return try await race.run(operation)
}

private final class AgentInactivityTimeoutRace<Value: Sendable>: @unchecked Sendable {
    private let lock = NSLock()
    private let seconds: Double
    private var continuation: CheckedContinuation<Value, Error>?
    private var pendingResult: Result<Value, Error>?
    private var operationTask: Task<Void, Never>?
    private var timerTask: Task<Void, Never>?
    private var timerGeneration = 0
    private var isResolved = false

    init(seconds: Double) { self.seconds = seconds }

    func run(
        _ operation: @escaping @Sendable (@escaping @Sendable () -> Void) async throws -> Value
    ) async throws -> Value {
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                install(continuation)
                touch()
                let task = Task { [self] in
                    do {
                        let value = try await operation { self.touch() }
                        self.resolve(.success(value))
                    } catch {
                        self.resolve(.failure(error))
                    }
                }
                installOperation(task)
            }
        } onCancel: {
            resolve(.failure(CancellationError()))
        }
    }

    private func install(_ value: CheckedContinuation<Value, Error>) {
        lock.lock()
        if let pendingResult {
            self.pendingResult = nil
            lock.unlock()
            value.resume(with: pendingResult)
        } else {
            continuation = value
            lock.unlock()
        }
    }

    private func installOperation(_ task: Task<Void, Never>) {
        lock.lock()
        if isResolved {
            lock.unlock()
            task.cancel()
        } else {
            operationTask = task
            lock.unlock()
        }
    }

    private func touch() {
        lock.lock()
        guard !isResolved else { lock.unlock(); return }
        timerGeneration += 1
        let generation = timerGeneration
        let previous = timerTask
        let seconds = seconds
        timerTask = Task { [weak self] in
            do {
                try await Task.sleep(for: .seconds(seconds))
                self?.timeout(generation: generation)
            } catch {
                // Replaced by a later activity timer.
            }
        }
        lock.unlock()
        previous?.cancel()
    }

    private func timeout(generation: Int) {
        lock.lock()
        guard !isResolved, generation == timerGeneration else { lock.unlock(); return }
        lock.unlock()
        resolve(.failure(AgentRuntimeError.timeout))
    }

    private func resolve(_ result: Result<Value, Error>) {
        lock.lock()
        guard !isResolved else { lock.unlock(); return }
        isResolved = true
        let continuation = continuation
        self.continuation = nil
        if continuation == nil { pendingResult = result }
        let operationTask = operationTask
        let timerTask = timerTask
        self.operationTask = nil
        self.timerTask = nil
        lock.unlock()
        operationTask?.cancel()
        timerTask?.cancel()
        continuation?.resume(with: result)
    }
}

private final class AgentModelAttemptTelemetry: @unchecked Sendable {
    private let lock = NSLock()
    private let startedAt = Date()
    private var headersReceived = false
    private var firstActivityAt: Date?
    private var lastActivityAt: Date?
    private var activityEvents = 0
    private var activityBytes = 0
    private var completionObserved = false

    func record(_ event: AgentModelStreamEvent) {
        let now = Date()
        lock.lock()
        firstActivityAt = firstActivityAt ?? now
        lastActivityAt = now
        switch event {
        case let .activity(bytes):
            activityEvents += 1
            activityBytes += max(0, bytes)
        case .responseCreated:
            headersReceived = true
        case .completed:
            completionObserved = true
        case .textDelta, .toolCallDelta:
            break
        }
        lock.unlock()
    }

    var detail: String {
        lock.lock()
        defer { lock.unlock() }
        let first = firstActivityAt.map { String(format: "%.1fs", $0.timeIntervalSince(startedAt)) } ?? "none"
        let last = lastActivityAt.map { String(format: "%.1fs", $0.timeIntervalSince(startedAt)) } ?? "none"
        return "headers=\(headersReceived), first=\(first), last=\(last), chunks=\(activityEvents), bytes=\(activityBytes), completed=\(completionObserved)"
    }
}

/// Races an operation against a deadline without waiting for a losing child task to unwind.
///
/// A throwing task group cannot provide this guarantee: its scope waits for every child even
/// after `cancelAll()`. Some provider transports only observe cancellation after their socket
/// returns, which used to leave an Agent Run durably stuck at `model_request`. The losing task is
/// still cancelled, but the caller is released immediately so it can persist a terminal state.
public func withAgentTimeout<T: Sendable>(
    seconds: Double,
    operation: @escaping @Sendable () async throws -> T
) async throws -> T {
    guard seconds > 0 else { throw AgentRuntimeError.timeout }
    let race = AgentTimeoutRace<T>()
    return try await withTaskCancellationHandler {
        try await withCheckedThrowingContinuation { continuation in
            race.install(continuation)
            let operationTask = Task {
                do {
                    race.resolve(.success(try await operation()))
                } catch {
                    race.resolve(.failure(error))
                }
            }
            let timeoutTask = Task {
                do {
                    try await Task.sleep(for: .seconds(seconds))
                    race.resolve(.failure(AgentRuntimeError.timeout))
                } catch {
                    // The winner cancels this timer. Its result was already delivered.
                }
            }
            race.installTasks(operationTask, timeoutTask)
        }
    } onCancel: {
        race.resolve(.failure(CancellationError()))
    }
}

private final class AgentTimeoutRace<Value: Sendable>: @unchecked Sendable {
    private let lock = NSLock()
    private var continuation: CheckedContinuation<Value, Error>?
    private var pendingResult: Result<Value, Error>?
    private var tasks: [Task<Void, Never>] = []
    private var isResolved = false

    func install(_ continuation: CheckedContinuation<Value, Error>) {
        lock.lock()
        if let pendingResult {
            self.pendingResult = nil
            lock.unlock()
            continuation.resume(with: pendingResult)
        } else {
            self.continuation = continuation
            lock.unlock()
        }
    }

    func installTasks(_ tasks: Task<Void, Never>...) {
        lock.lock()
        if isResolved {
            lock.unlock()
            tasks.forEach { $0.cancel() }
        } else {
            self.tasks = tasks
            lock.unlock()
        }
    }

    func resolve(_ result: Result<Value, Error>) {
        lock.lock()
        guard !isResolved else {
            lock.unlock()
            return
        }
        isResolved = true
        let continuation = continuation
        self.continuation = nil
        if continuation == nil {
            pendingResult = result
        }
        let tasks = tasks
        self.tasks.removeAll()
        lock.unlock()

        tasks.forEach { $0.cancel() }
        continuation?.resume(with: result)
    }
}
