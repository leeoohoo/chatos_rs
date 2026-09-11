import Foundation

public struct AgentRuntime: Sendable {
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
        contextProvider: AgentMemoryContextProvider? = nil,
        shouldPause: @escaping @Sendable () async -> Bool = { false },
        onModelStreamEvent: @escaping @Sendable (AgentModelStreamEvent) async -> Void = { _ in },
        record: @escaping Recorder = { _, _ in }
    ) async throws -> AgentRunCheckpoint {
        try policy.validate()
        guard initial.scope == scope else { throw AgentRuntimeError.scopeMismatch }
        guard Set(tools.map(\.name)).count == tools.count else { throw AgentRuntimeError.invalidResponse }
        if let inFlight = initial.inFlightCallID, !initial.pendingCalls.contains(where: { $0.id == inFlight }) {
            throw AgentRuntimeError.invalidResponse
        }
        var state = initial
        if state.status == .completed { return state }
        let started = Date()
        let elapsedBefore = state.elapsedSeconds
        let registry = Dictionary(uniqueKeysWithValues: tools.map { ($0.name, $0) })
        let contextPolicy = policy.context ?? AgentContextPolicy()
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
        state.status = .running
        state.stopReason = nil
        try await emit("started", "Agent 运行开始 / 恢复")
        do {
            while true {
                try Task.checkCancellation()
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
                        let synced = try await contextProvider.prepare(checkpoint: snapshot(state), tools: tools, policy: contextPolicy,
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
                            outcome = try await withTimeout(seconds: remainingTime()) { try await execute(call) }
                        } catch {
                            if definition.effect == .billable || definition.effect == .write {
                                state.status = .needsReview
                                try await emit("needs_review", "\(call.name)：\(error.localizedDescription)")
                                return snapshot(state)
                            }
                            if error is CancellationError { throw error }
                            outcome = .failure(error.localizedDescription)
                        }
                    } else { outcome = .failure("工具不可用：\(call.name)。请选择本次提供的工具。") }

                    // Results are bounded references/summaries, never media blobs or unbounded file contents.
                    if outcome.content.count > 16_000 { outcome.content = String(outcome.content.prefix(16_000)) + "\n[工具结果已截断，请缩小读取范围]" }
                    let signature = fingerprint(call)
                    if state.observations[signature] == outcome.content { outcome.madeProgress = false }
                    state.observations[signature] = outcome.content
                    state.receipts[call.id] = outcome
                    state.callFingerprints[call.id] = signature
                    state.inFlightCallID = nil
                    state.messages.append(.init(role: .tool, content: outcome.content, toolCallID: call.id))
                    state.pendingCalls.removeFirst()
                    if outcome.madeProgress && !outcome.isError { state.noProgressRounds = 0 }
                    if definition?.effect == .terminal && !outcome.isError { state.completionResult = outcome.content }
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
                    let prepared = try await contextProvider.prepare(checkpoint: snapshot(state), tools: tools, policy: contextPolicy,
                        deadline: deadline, shouldPause: shouldPause, record: record)
                    state = prepared.checkpoint; messages = prepared.messages
                } else {
                    guard state.memory == nil else { throw AgentContextError.unavailable }
                    messages = state.messages
                    guard try AgentContextBudget.estimate(messages: messages, tools: tools) <= contextPolicy.hardInputLimit else {
                        throw AgentContextError.budgetExceeded
                    }
                }
                var response: AgentMessage?
                var recoveredOverflow = false
                for attempt in 0...policy.maximumRequestRetries {
                    try Task.checkCancellation()
                    if await shouldPause() { throw CancellationError() }
                    guard state.modelCalls < policy.maximumModelCalls else { break }
                    state.modelCalls += 1
                    try await emit("model_request", "模型调用 \(state.modelCalls) / \(policy.maximumModelCalls)")
                    let requestMessages = messages
                    let timeout = min(Double(policy.requestTimeoutSeconds), remainingTime())
                    do {
                        response = try await withTimeout(seconds: timeout) {
                            try await model.stream(messages: requestMessages, tools: tools, timeout: timeout,
                                                   onEvent: onModelStreamEvent)
                        }
                        break
                    } catch {
                        if case AgentRuntimeError.contextOverflow = error, let contextProvider,
                           !recoveredOverflow, attempt < policy.maximumRequestRetries, state.modelCalls < policy.maximumModelCalls {
                            let prepared = try await contextProvider.prepare(checkpoint: snapshot(state), tools: tools, policy: contextPolicy,
                                forceCompaction: true, deadline: deadline, shouldPause: shouldPause, record: record)
                            state = prepared.checkpoint; messages = prepared.messages; recoveredOverflow = true
                            continue
                        }
                        guard attempt < policy.maximumRequestRetries, AgentRuntimeError.isTransient(error) else { throw error }
                        let retryNumber = attempt + 1
                        let delaySeconds = Self.retryDelaySeconds(forRetry: retryNumber)
                        guard remainingTime() > Double(delaySeconds) else { throw AgentRuntimeError.timeout }
                        try await emit(
                            "model_retry",
                            "暂时性模型请求失败；第 \(retryNumber) / \(policy.maximumRequestRetries) 次重试将在 \(delaySeconds) 秒后开始"
                        )
                        try await retrySleeper(.seconds(delaySeconds))
                    }
                }
                guard let response else { continue }
                guard response.role == .assistant, response.toolCalls.count <= 32,
                      Set(response.toolCalls.map(\.id)).count == response.toolCalls.count,
                      response.toolCalls.allSatisfy({ !$0.id.isEmpty && state.receipts[$0.id] == nil }) else { throw AgentRuntimeError.invalidResponse }
                state.noProgressRounds += 1
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
}

private func withTimeout<T: Sendable>(seconds: Double, operation: @escaping @Sendable () async throws -> T) async throws -> T {
    guard seconds > 0 else { throw AgentRuntimeError.timeout }
    return try await withThrowingTaskGroup(of: T.self) { group in
        group.addTask { try await operation() }
        group.addTask { try await Task.sleep(for: .seconds(seconds)); throw AgentRuntimeError.timeout }
        defer { group.cancelAll() }
        guard let value = try await group.next() else { throw CancellationError() }
        return value
    }
}
