import Foundation
import XCTest
@testable import ChatOSAgentRuntime

final class AgentCurrentTurnContextTests: XCTestCase {
    func testCurrentTurnInstructionsSurviveComposeExclusionWithOrWithoutSummary() throws {
        let (base, scope) = try fixture()
        var checkpoint = base
        checkpoint.messages.append(.init(role: .system, content: "product Skill router"))
        let memory = AgentMemoryCheckpoint(scope: scope, pinnedMessageCount: 2)
        for blocks: [AgentMemoryContextBlock] in [[], [.init(blockType: "thread_summary", text: "earlier work")]] {
            let messages = try AgentContextAssembler.assemble(
                checkpoint: checkpoint, memory: memory,
                context: .init(blocks: blocks, recentRecords: [
                    .init(id: scope.recordID(at: 0), message: checkpoint.messages[0]),
                ])
            )
            XCTAssertEqual(messages.filter { checkpoint.messages.contains($0) }, checkpoint.messages)
        }
    }

    func testResumedCurrentTurnKeepsExactResponsesOutputAndToolResultsOnce() throws {
        var (checkpoint, scope) = try fixture()
        let call = AgentToolCall(id: "write-once", name: "write", arguments: "{}")
        let response = AgentMessage(
            role: .assistant, content: "written", toolCalls: [call],
            responseOutputJSON: Data(#"[{"type":"reasoning","encrypted_content":"retained"},{"type":"function_call","call_id":"write-once","name":"write","arguments":"{}"}]"#.utf8),
            usage: .init(inputTokens: 12, outputTokens: 4, requests: 1)
        )
        checkpoint.messages += [response, .init(role: .tool, content: "write receipt", toolCallID: call.id)]
        let memory = AgentMemoryCheckpoint(scope: scope, pinnedMessageCount: 2)
        for includeCurrentRecords in [false, true] {
            let records = includeCurrentRecords ? checkpoint.messages.indices.map {
                AgentMemoryContextRecord(id: scope.recordID(at: $0), message: checkpoint.messages[$0])
            } : []
            let messages = try AgentContextAssembler.assemble(
                checkpoint: checkpoint, memory: memory,
                context: .init(blocks: [.init(blockType: "thread_summary", text: "earlier work")], recentRecords: records)
            )
            XCTAssertEqual(Array(messages.suffix(3)), Array(checkpoint.messages.suffix(3)))
            XCTAssertEqual(messages.filter { $0.toolCalls.contains(call) }.count, 1)
            XCTAssertEqual(messages.filter { $0.toolCallID == call.id }.count, 1)
        }
    }

    func testPinnedRouterAndCurrentTriggerAreNotDuplicated() throws {
        var (checkpoint, scope) = try fixture()
        checkpoint.messages.append(.init(role: .system, content: "pinned router"))
        let memory = AgentMemoryCheckpoint(scope: scope, pinnedMessageCount: 3)
        let messages = try AgentContextAssembler.assemble(
            checkpoint: checkpoint, memory: memory, context: .init(blocks: [], recentRecords: [])
        )
        XCTAssertEqual(messages, [checkpoint.messages[0], checkpoint.messages[2], checkpoint.messages[1]])
    }

    func testCurrentTurnKeepsAttachmentFromAuthoritativeCheckpoint() throws {
        var (checkpoint, scope) = try fixture()
        checkpoint.messages[1].attachments = [.init(
            name: "reference.png", mimeType: "image/png", kind: .image,
            localFileURL: URL(fileURLWithPath: "/tmp/current-turn-reference.png")
        )]
        let messages = try AgentContextAssembler.assemble(
            checkpoint: checkpoint, memory: .init(scope: scope, pinnedMessageCount: 2),
            context: .init(blocks: [], recentRecords: [])
        )
        XCTAssertEqual(messages, checkpoint.messages)
    }

    func testStartupWithHookContextAndLateSkillRouterNeedsNoSummary() throws {
        var (checkpoint, scope) = try fixture()
        checkpoint.messages.insert(.init(role: .system, content: "project hook context"), at: 1)
        checkpoint.messages.append(.init(role: .system, content: "product Skill router"))
        let messages = try AgentContextAssembler.assemble(
            checkpoint: checkpoint, memory: .init(scope: scope, pinnedMessageCount: 3),
            context: .init(blocks: [], recentRecords: [])
        )
        XCTAssertEqual(messages, checkpoint.messages)
    }

    func testEarlierTurnStillRequiresHistoryWhileLiveTurnComesFromCheckpoint() throws {
        var (checkpoint, scope) = try fixture()
        checkpoint.messages += [.init(role: .assistant, content: "earlier answer"), .init(role: .user, content: "continue")]
        let memory = AgentMemoryCheckpoint(scope: scope, pinnedMessageCount: 2)
        XCTAssertThrowsError(try AgentContextAssembler.assemble(
            checkpoint: checkpoint, memory: memory, context: .init(blocks: [], recentRecords: [])
        ))
        let messages = try AgentContextAssembler.assemble(
            checkpoint: checkpoint, memory: memory,
            context: .init(blocks: [], recentRecords: [
                .init(id: scope.recordID(at: 2), message: checkpoint.messages[2]),
            ])
        )
        XCTAssertEqual(messages.last, checkpoint.messages.last)
        XCTAssertEqual(messages.filter { $0 == checkpoint.messages[1] }.count, 1)
        XCTAssertEqual(messages.filter { $0 == checkpoint.messages[2] }.count, 1)
    }

    func testInvalidLiveToolExchangesAreRejectedInsteadOfDiscarded() throws {
        let (base, scope) = try fixture()
        let memory = AgentMemoryCheckpoint(scope: scope, pinnedMessageCount: 2)
        let call = AgentMessage(role: .assistant, toolCalls: [.init(id: "call-1", name: "write", arguments: "{}")])
        let result = AgentMessage(role: .tool, content: "receipt", toolCallID: "call-1")
        for tail in [[call], [result], [call, result, result], [call, result, call, result]] {
            var checkpoint = base
            checkpoint.messages += tail
            XCTAssertThrowsError(try AgentContextAssembler.assemble(
                checkpoint: checkpoint, memory: memory,
                context: .init(blocks: [.init(blockType: "thread_summary", text: "summary cannot hide corruption")], recentRecords: [])
            ))
        }
    }

    func testRuntimeResumesExcludedLiveTurnWithoutRepeatingCompletedTool() async throws {
        let (checkpoint, scope) = try fixture()
        let service = ExcludingCurrentTurnMemory()
        let provider = AgentMemoryContextProvider(scope: scope, service: service)
        var initial = try provider.bind(checkpoint)
        initial.messages.append(.init(role: .system, content: "product Skill router"))
        let model = CurrentTurnResumeModel()
        let effects = CurrentTurnEffects()
        let first = try await AgentRuntime().run(
            checkpoint: initial, scope: scope.runtimeScope, policy: .init(), model: model,
            tools: runtimeTestTools, execute: { call in await effects.execute(call) },
            contextProvider: provider, shouldPause: { await effects.hasWritten() }
        )
        XCTAssertEqual(first.status, .paused)
        XCTAssertEqual(first.modelCalls, 1)
        let restored = try JSONDecoder().decode(AgentRunCheckpoint.self, from: JSONEncoder().encode(first))
        let resumed = try await AgentRuntime().run(
            checkpoint: restored, scope: scope.runtimeScope, policy: .init(), model: model,
            tools: runtimeTestTools, execute: { call in await effects.execute(call) }, contextProvider: provider
        )
        XCTAssertEqual(resumed.status, .completed)
        XCTAssertEqual(resumed.modelCalls, 2)
        let calls = await effects.calls
        XCTAssertEqual(calls, ["write-once", "finish"])
        let requests = await model.requests
        XCTAssertEqual(requests.count, 2)
        if requests.count == 2 {
            XCTAssertEqual(requests[1], first.messages)
        }
        XCTAssertEqual(resumed.memory?.syncedMessageCount, resumed.messages.count)
    }

    private func fixture() throws -> (AgentRunCheckpoint, AgentMemoryScope) {
        let checkpoint = AgentRunCheckpoint(scope: "current-turn-test", messages: [
            .init(role: .system, content: "Only authorized tools"), .init(role: .user, content: "Do this once"),
        ])
        let scope = try AgentMemoryScope(
            tenantID: "user-a", agentID: "agent-a", projectID: nil,
            runID: checkpoint.id, runtimeScope: checkpoint.scope
        )
        return (checkpoint, scope)
    }
}

private actor ExcludingCurrentTurnMemory: AgentMemoryServicing {
    private var entries: [AgentMemoryEntry] = []
    func ensureThread() {}
    func sync(_ entries: [AgentMemoryEntry], reconciling: Bool) { self.entries += entries }
    func compose(_ request: AgentMemoryComposeRequest) -> AgentMemoryContext {
        .init(blocks: [], recentRecords: entries.filter { $0.conversationTurnID != request.currentTurnID }.map {
            .init(id: $0.id, message: $0.message)
        })
    }
}

private actor CurrentTurnResumeModel: AgentModelClient {
    nonisolated let usesServerSideCompaction = true
    nonisolated let memoryCapabilitySnapshot = try? AgentModelCapabilitySnapshot(
        revision: "current-turn-test-v1", contextWindowTokens: 200_000, outputReserveTokens: 30_000,
        protocolOverheadEstimatorID: "responses-json-v1", tokenizerID: nil
    )
    var requests: [[AgentMessage]] = []
    func complete(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval) -> AgentMessage {
        requests.append(messages)
        let first = requests.count == 1
        return .init(role: .assistant, toolCalls: [.init(
            id: first ? "write-once" : "finish", name: first ? "work" : "finish",
            arguments: first ? #"{"step":1}"# : "{}"
        )])
    }
}

private actor CurrentTurnEffects {
    var calls: [String] = []
    func hasWritten() -> Bool { calls.contains("write-once") }
    func execute(_ call: AgentToolCall) -> AgentToolOutcome {
        calls.append(call.id)
        return .init(call.id == "write-once" ? "write receipt" : "finished")
    }
}
