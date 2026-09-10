import Foundation
import XCTest
@testable import ChatOSAgentRuntime

final class AgentLoopSafetyTests: XCTestCase {
    func testErrorResultsAreReturnedWithOriginalIDsThenModelCanRepair() async throws {
        let model = ScriptModel([
            .init(role: .assistant, toolCalls: [.init(id: "bad", name: "work", arguments: #"{"step":true}"#), .init(id: "unknown", name: "pay", arguments: "{}")]),
            .init(role: .assistant, toolCalls: [.init(id: "finish", name: "finish", arguments: "{}")]),
        ])
        let result = try await run(model) { call in
            XCTAssertEqual(call.name, "finish", "Invalid/unknown tools cannot execute")
            return .init("done")
        }
        XCTAssertEqual(result.status, .completed)
        let second = await model.requests[1]
        XCTAssertEqual(second.filter { $0.role == .tool }.map(\.toolCallID), ["bad", "unknown"])
        XCTAssertEqual(result.receipts["bad"]?.isError, true)
    }

    func testMixedTerminalBatchExecutesNeitherTool() async throws {
        let model = ScriptModel([
            .init(role: .assistant, toolCalls: [.init(id: "a", name: "finish", arguments: "{}"), .init(id: "b", name: "work", arguments: #"{"step":1}"#)]),
            .init(role: .assistant, toolCalls: [.init(id: "c", name: "finish", arguments: "{}")]),
        ])
        let result = try await run(model) { call in XCTAssertEqual(call.id, "c"); return .init("done") }
        XCTAssertEqual(result.status, .completed)
        XCTAssertEqual(result.receipts["a"]?.isError, true)
        XCTAssertEqual(result.receipts["b"]?.isError, true)
    }

    func testInFlightBillableOperationIsNeverReplayed() async throws {
        var checkpoint = base
        checkpoint.pendingCalls = [.init(id: "paid", name: "generate", arguments: "{}")]
        checkpoint.inFlightCallID = "paid"
        let model = ScriptModel([])
        let result = try await AgentRuntime().run(checkpoint: checkpoint, scope: base.scope, policy: .init(), model: model,
            tools: [.init(name: "generate", description: "paid", schema: Data(#"{"type":"object"}"#.utf8), effect: .billable)],
            execute: { _ in XCTFail("Cannot replay"); return .init("bad") })
        XCTAssertEqual(result.status, .needsReview)
        XCTAssertEqual(result.modelCalls, 0)
    }

    func testRetriesConsumeBudget() async throws {
        let model = FailingModel()
        var policy = AgentRunPolicy(); policy.maximumModelCalls = 2
        let result = try await AgentRuntime().run(checkpoint: base, scope: base.scope, policy: policy,
            model: model, tools: runtimeTestTools, execute: { _ in .init("unused") })
        XCTAssertEqual(result.modelCalls, 2)
        XCTAssertEqual(result.status, .limitReached)
    }

    func testSettingsPersistAndValidateOverridesAndWindowBudget() throws {
        let name = "AgentSettingsTests.\(UUID())"
        defer { UserDefaults(suiteName: name)?.removePersistentDomain(forName: name) }
        let store = AgentSettingsStore(suiteName: name)
        var preferences = AgentRuntimePreferences()
        preferences.approvalMaximumCalls = 77; preferences.storyMaximumCalls = 800
        preferences.global.context = .init()
        try store.save(preferences)
        let restored = try store.load()
        XCTAssertEqual(restored.effective(.approval).maximumModelCalls, 77)
        XCTAssertEqual(restored.effective(.story).maximumModelCalls, 800)
        preferences.global.context!.outputReserveTokens = preferences.global.context!.windowTokens
        XCTAssertThrowsError(try store.save(preferences))
        XCTAssertEqual(try store.load(), restored, "Invalid changes do not overwrite saved settings")
    }

    func testMalformedStoredPreferencesDoNotSilentlyFallback() throws {
        let name = "AgentSettingsTests.\(UUID())"
        defer { UserDefaults(suiteName: name)?.removePersistentDomain(forName: name) }
        UserDefaults(suiteName: name)?.set(Data("invalid".utf8), forKey: "chatos.agent-runtime.settings.v1")
        XCTAssertThrowsError(try AgentSettingsStore(suiteName: name).load())
    }

    func testRepeatedIdenticalToolWorkPausesInsteadOfUsing600Calls() async throws {
        let model = ScriptModel((1...10).map { .init(role: .assistant, toolCalls: [.init(id: "same-\($0)", name: "work", arguments: #"{"step":1}"#)]) })
        var policy = AgentRunPolicy(); policy.maximumNoProgressRounds = 2
        let result = try await AgentRuntime().run(checkpoint: base, scope: base.scope, policy: policy, model: model,
            tools: runtimeTestTools, execute: { _ in .init("unchanged") })
        XCTAssertEqual(result.status, .paused)
        XCTAssertEqual(result.modelCalls, 3)
    }

    func testPauseDoesNotCallModel() async throws {
        let model = ScriptModel([])
        let result = try await AgentRuntime().run(checkpoint: base, scope: base.scope, policy: .init(), model: model,
            tools: runtimeTestTools, execute: { _ in .init("unused") }, shouldPause: { true })
        XCTAssertEqual(result.status, .paused)
        XCTAssertEqual(result.modelCalls, 0)
    }

    func testRunDeadlineCancelsCooperativeModelRequest() async throws {
        var policy = AgentRunPolicy(); policy.runTimeoutSeconds = 10
        var checkpoint = base; checkpoint.elapsedSeconds = 9.95
        let result = try await AgentRuntime().run(checkpoint: checkpoint, scope: base.scope, policy: policy,
            model: SlowModel(), tools: runtimeTestTools, execute: { _ in .init("unused") })
        XCTAssertEqual(result.status, .failed)
        XCTAssertLessThanOrEqual(result.modelCalls, 1)
        XCTAssertLessThan(result.elapsedSeconds, 11)
    }

    private var base: AgentRunCheckpoint { .init(scope: "test", messages: [.init(role: .user, content: "test")]) }
    private func run(_ model: ScriptModel, execute: @escaping AgentRuntime.Executor) async throws -> AgentRunCheckpoint {
        try await AgentRuntime().run(checkpoint: base, scope: base.scope, policy: .init(), model: model, tools: runtimeTestTools, execute: execute)
    }
}

private actor ScriptModel: AgentModelClient {
    var responses: [AgentMessage]
    var requests: [[AgentMessage]] = []
    init(_ responses: [AgentMessage]) { self.responses = responses }
    func complete(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval) throws -> AgentMessage {
        requests.append(messages)
        guard !responses.isEmpty else { throw AgentRuntimeError.invalidResponse }
        return responses.removeFirst()
    }
}
private struct FailingModel: AgentModelClient {
    func complete(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval) throws -> AgentMessage { throw AgentRuntimeError.provider(503) }
}
private struct SlowModel: AgentModelClient {
    func complete(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval) async throws -> AgentMessage {
        try await Task.sleep(for: .seconds(2))
        return .init(role: .assistant, content: "late")
    }
}
