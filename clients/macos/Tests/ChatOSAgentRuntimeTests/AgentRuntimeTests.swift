import Foundation
import XCTest
@testable import ChatOSAgentRuntime
import ChatOSCore

final class AgentRuntimeTests: XCTestCase {
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
