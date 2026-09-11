import Foundation
import XCTest
@testable import ChatOSAgentRuntime

final class AgentRuntimeTests: XCTestCase {
    func testDefaultBudgetIs600() throws {
        let settings = AgentRuntimePreferences()
        XCTAssertEqual(settings.effective(.story).maximumModelCalls, 600)
        XCTAssertEqual(settings.effective(.approval).maximumModelCalls, 600)
        XCTAssertEqual(settings.global.maximumRequestRetries, 5)
        let context = try XCTUnwrap(settings.global.context ?? AgentContextPolicy())
        XCTAssertEqual(context.windowTokens, 250_000)
        XCTAssertEqual(context.outputReserveTokens, 30_000)
        XCTAssertEqual(context.compactionThresholdTokens, 220_000)
        XCTAssertEqual(context.maximumCompactionPasses, 8)
        XCTAssertEqual(context.summaryPollSeconds, 10)
        try settings.validate()
    }

    func testLegacyRetryDefaultMigratesOnceWithoutResettingContextSettings() throws {
        let name = "AgentRetryMigrationTests.\(UUID())"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: name))
        defer { defaults.removePersistentDomain(forName: name) }
        var old = AgentRuntimePreferences()
        old.global.maximumRequestRetries = 2
        var context = AgentContextPolicy()
        context.windowTokens = 2_000_000
        context.outputReserveTokens = 30_000
        context.compactionThresholdTokens = 200_000
        old.global.context = context
        defaults.set(try JSONEncoder().encode(old), forKey: "chatos.agent-runtime.settings.v1")

        let migrated = try AgentSettingsStore(suiteName: name).load()
        XCTAssertEqual(migrated.global.maximumRequestRetries, 5)
        XCTAssertEqual(migrated.global.context?.windowTokens, 2_000_000)

        var explicitlyChanged = migrated
        explicitlyChanged.global.maximumRequestRetries = 2
        try AgentSettingsStore(suiteName: name).save(explicitlyChanged)
        XCTAssertEqual(try AgentSettingsStore(suiteName: name).load().global.maximumRequestRetries, 2)
    }

    func testContextEstimateReturnsApproximateTokensRatherThanRawBytes() throws {
        let messages = [AgentMessage(role: .user, content: String(repeating: "x", count: 4_000))]
        let estimate = try AgentContextBudget.estimate(messages: messages, tools: [])
        XCTAssertGreaterThan(estimate, 900)
        XCTAssertLessThan(estimate, 1_200)
    }

    func testDeterministicCompletionCheckFinishesWithoutAnotherModelCall() async throws {
        let checkpoint = AgentRunCheckpoint(scope: "test", messages: [.init(role: .user, content: "work")])
        let model = CompletionCheckModel()
        let result = try await AgentRuntime().run(
            checkpoint: checkpoint, scope: "test", policy: .init(), model: model, tools: [],
            execute: { _ in .failure("unexpected") }, completionCheck: { "validated result" }
        )
        XCTAssertEqual(result.status, .completed)
        XCTAssertEqual(result.result, "validated result")
        XCTAssertEqual(result.modelCalls, 0)
        let callCount = await model.callCount
        XCTAssertEqual(callCount, 0)
    }
}

private actor CompletionCheckModel: AgentModelClient {
    var callCount = 0
    func complete(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval) async throws -> AgentMessage {
        callCount += 1
        return .init(role: .assistant)
    }
}
