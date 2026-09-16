import Foundation
import XCTest
@testable import ChatOSAgentRuntime

final class AgentMemoryContextTests: XCTestCase {
    func testProjectChatMemoryIsPrivateToAgentAndProject() throws {
        let runID = UUID()
        let first = try AgentMemoryScope(
            tenantID: "user-a",
            agentID: "agent-a",
            projectID: "project-1",
            runID: runID,
            runtimeScope: "account:user-a:project:project-1:agent:agent-a"
        )
        let second = try AgentMemoryScope(
            tenantID: "user-a",
            agentID: "agent-b",
            projectID: "project-1",
            runID: runID,
            runtimeScope: "account:user-a:project:project-1:agent:agent-b"
        )
        XCTAssertEqual(first.subjectID, "agent_project:agent-a:project-1")
        XCTAssertEqual(second.subjectID, "agent_project:agent-b:project-1")
        XCTAssertNotEqual(first.subjectID, second.subjectID)
        XCTAssertNotEqual(first.threadID, second.threadID)
        XCTAssertNil(first.includeSubjectMemory)
    }

    func test600ResponsesCallsComposeMemoryOnceAndKeepAuditHistoryComplete() async throws {
        let (checkpoint, scope) = try fixture()
        let memory = TestMemory(scope: scope)
        let provider = AgentMemoryContextProvider(scope: scope, service: memory)
        let model = CountingModel(finishAt: 600)
        var policy = AgentRunPolicy(); policy.context = contextPolicy
        let result = try await AgentRuntime().run(checkpoint: provider.bind(checkpoint), scope: checkpoint.scope,
            policy: policy, model: model, tools: tools, execute: { call in .init(call.arguments + String(repeating: "x", count: 150)) },
            contextProvider: provider)
        XCTAssertEqual(result.status, .completed)
        XCTAssertEqual(result.modelCalls, 600)
        XCTAssertEqual(result.receipts.count, 600)
        XCTAssertEqual(result.messages.count, 1_202)
        XCTAssertEqual(result.memory?.syncedMessageCount, result.messages.count)
        let composeCalls = await memory.composeCalls
        XCTAssertEqual(composeCalls, 1)
        let stored = await memory.entries.count
        XCTAssertEqual(stored, result.messages.count)
    }

    func testModelCallLimitFlushesAssistantToolCallAndToolResultToMemory() async throws {
        let (checkpoint, scope) = try fixture()
        let memory = TestMemory(scope: scope)
        let provider = AgentMemoryContextProvider(scope: scope, service: memory)
        let model = CountingModel(finishAt: 2)
        var policy = AgentRunPolicy()
        policy.maximumModelCalls = 1
        policy.context = contextPolicy

        let result = try await AgentRuntime().run(
            checkpoint: provider.bind(checkpoint), scope: checkpoint.scope,
            policy: policy, model: model, tools: tools,
            execute: { call in .init("result-for-\(call.id)") }, contextProvider: provider
        )

        XCTAssertEqual(result.status, .limitReached)
        XCTAssertEqual(result.messages.count, 4)
        XCTAssertEqual(result.memory?.syncedMessageCount, result.messages.count)
        let stored = await memory.entries
        XCTAssertEqual(stored.map(\.message), result.messages)
        XCTAssertEqual(stored[2].message.toolCalls.first?.id, "call-1")
        XCTAssertEqual(stored[3].message.toolCallID, "call-1")
        XCTAssertEqual(stored[3].message.content, "result-for-call-1")
    }

    func testComposeUsesBlocksRecentRecordsAndStickyTaskLikeTaskRunner() throws {
        var (checkpoint, scope) = try fixture()
        checkpoint.messages += [
            .init(role: .assistant, toolCalls: [.init(id: "a", name: "read", arguments: "{}"), .init(id: "b", name: "read", arguments: "{}")]),
            .init(role: .tool, content: "A", toolCallID: "a"), .init(role: .tool, content: "B", toolCallID: "b"),
            .init(role: .assistant, content: "next"),
        ]
        let memory = AgentMemoryCheckpoint(scope: scope, pinnedMessageCount: 2)
        let records = (2..<checkpoint.messages.count).map {
            AgentMemoryContextRecord(id: scope.recordID(at: $0), message: checkpoint.messages[$0])
        }
        let result = try AgentContextAssembler.assemble(checkpoint: checkpoint, memory: memory,
            context: .init(blocks: [.init(blockType: "thread_summary", text: "previous work")],
                           recentRecords: records))
        XCTAssertEqual(result[0], checkpoint.messages[0])
        XCTAssertEqual(result[1], .init(role: .system, content: "[thread_summary]\nprevious work"))
        XCTAssertEqual(result.filter { $0.role == .tool }.map(\.toolCallID), ["a", "b"])
        XCTAssertEqual(result[result.count - 2].content, "next")
        XCTAssertEqual(result.last, checkpoint.messages[1], "Current task contract must remain sticky")
    }

    func testRejectsMissingHistoryWithoutSummaryAndUnknownRecords() throws {
        var (checkpoint, scope) = try fixture()
        checkpoint.messages.append(.init(role: .assistant, content: "must not be dropped"))
        let memory = AgentMemoryCheckpoint(scope: scope, pinnedMessageCount: 2)
        for context in [AgentMemoryContext(blocks: [], recentRecords: []),
                        .init(blocks: [.init(blockType: "thread_summary", text: "summary")],
                              recentRecords: [.init(id: "another-run-record", message: checkpoint.messages[2])])] {
            XCTAssertThrowsError(try AgentContextAssembler.assemble(checkpoint: checkpoint, memory: memory, context: context))
        }
    }

    func testFiltersOrphanAndUnfinishedToolCallsLikeSharedRuntime() throws {
        let (base, scope) = try fixture()
        let memory = AgentMemoryCheckpoint(scope: scope, pinnedMessageCount: 2)
        for message in [AgentMessage(role: .tool, content: "orphan", toolCallID: "unknown"),
                        .init(role: .assistant, toolCalls: [.init(id: "missing-result", name: "work", arguments: "{}")])] {
            var checkpoint = base; checkpoint.messages.append(message)
            let result = try AgentContextAssembler.assemble(checkpoint: checkpoint, memory: memory,
                context: .init(blocks: [.init(blockType: "thread_summary", text: "summary")],
                               recentRecords: [.init(id: scope.recordID(at: 2), message: message)]))
            XCTAssertFalse(result.contains(message))
        }
    }

    func testLargeInitialComposeIsSentToResponsesCompaction() async throws {
        var (checkpoint, scope) = try fixture()
        checkpoint.messages[1].content = String(repeating: "故事", count: 2_000)
        let memory = TestMemory(scope: scope)
        let provider = AgentMemoryContextProvider(scope: scope, service: memory)
        let model = CountingModel(finishAt: 1)
        var policy = AgentRunPolicy(); policy.context = contextPolicy
        let result = try await AgentRuntime().run(checkpoint: provider.bind(checkpoint), scope: checkpoint.scope,
            policy: policy, model: model, tools: tools, execute: { _ in .init("ok") }, contextProvider: provider)
        XCTAssertEqual(result.status, .completed)
        XCTAssertEqual(result.modelCalls, 1)
        XCTAssertEqual(result.memory?.syncedMessageCount, result.messages.count)
    }

    func testLostSyncAcknowledgementReconcilesInsteadOfRewriting() async throws {
        let (checkpoint, scope) = try fixture()
        let memory = TestMemory(scope: scope, loseSyncAck: true)
        let provider = AgentMemoryContextProvider(scope: scope, service: memory)
        let model = CountingModel(finishAt: 1)
        let first = try await AgentRuntime().run(checkpoint: provider.bind(checkpoint), scope: checkpoint.scope,
            policy: .init(), model: model, tools: tools, execute: { _ in .init("ok") }, contextProvider: provider)
        XCTAssertEqual(first.status, .paused)
        XCTAssertEqual(first.memory?.syncInFlightEnd, 2)
        XCTAssertEqual(first.modelCalls, 0)
        let restored = try JSONDecoder().decode(AgentRunCheckpoint.self, from: JSONEncoder().encode(first))
        let resumed = try await AgentRuntime().run(checkpoint: restored, scope: checkpoint.scope,
            policy: .init(), model: model, tools: tools, execute: { _ in .init("ok") }, contextProvider: provider)
        XCTAssertEqual(resumed.status, .completed)
        let reconciliations = await memory.reconciliations
        XCTAssertEqual(reconciliations, 1)
        let writes = await memory.writes
        XCTAssertEqual(writes, 3, "Initial batch, assistant tool call, and tool result; initial batch is not resent")
    }

    func testTerminalCompletionSurvivesFinalSyncFailureWithoutAnotherModelOrToolCall() async throws {
        let (checkpoint, scope) = try fixture()
        let memory = TestMemory(scope: scope, failWriteNumber: 3)
        let provider = AgentMemoryContextProvider(scope: scope, service: memory)
        let model = CountingModel(finishAt: 1)
        let first = try await AgentRuntime().run(checkpoint: provider.bind(checkpoint), scope: checkpoint.scope,
            policy: .init(), model: model, tools: tools, execute: { _ in .init("finished") }, contextProvider: provider)
        XCTAssertEqual(first.status, .paused)
        XCTAssertEqual(first.completionResult, "finished")
        let resumed = try await AgentRuntime().run(checkpoint: first, scope: checkpoint.scope,
            policy: .init(), model: model, tools: tools, execute: { _ in XCTFail("Do not repeat terminal tool"); return .init("bad") }, contextProvider: provider)
        XCTAssertEqual(resumed.status, .completed)
        XCTAssertEqual(resumed.modelCalls, 1)
    }

    func testHistoryMutationAndWrongScopeFailBeforeNetwork() async throws {
        let (initial, scope) = try fixture()
        let memory = TestMemory(scope: scope)
        let provider = AgentMemoryContextProvider(scope: scope, service: memory)
        var checkpoint = try provider.bind(initial)
        checkpoint = try await provider.prepare(checkpoint: checkpoint, tools: tools, policy: contextPolicy,
            deadline: Date().addingTimeInterval(60), synchronizeOnly: true, record: { _, _ in }).checkpoint
        checkpoint.messages[1].content = "mutated"
        do {
            _ = try await provider.prepare(checkpoint: checkpoint, tools: tools, policy: contextPolicy,
                deadline: Date().addingTimeInterval(60), record: { _, _ in })
            XCTFail("Should fail")
        } catch { XCTAssertTrue(error is AgentContextPreparationFailure) }
        var other = initial; other.scope = "another-account"
        XCTAssertThrowsError(try provider.bind(other))
        let writes = await memory.writes
        XCTAssertEqual(writes, 1)
    }

    private var contextPolicy: AgentContextPolicy {
        var policy = AgentContextPolicy(); policy.windowTokens = 2_048; policy.outputReserveTokens = 512
        return policy
    }
    private func fixture() throws -> (AgentRunCheckpoint, AgentMemoryScope) {
        let checkpoint = AgentRunCheckpoint(scope: "account:story:version1", messages: [.init(role: .system, content: "Only authorized tools"), .init(role: .user, content: "Plan the story")])
        let scope = try AgentMemoryScope(tenantID: "user-a", profile: "story", projectID: UUID(), runID: checkpoint.id, runtimeScope: checkpoint.scope)
        return (checkpoint, scope)
    }
    private var tools: [AgentToolDefinition] { runtimeTestTools }
}

let runtimeTestTools: [AgentToolDefinition] = [
    .init(name: "work", description: "One step", schema: Data(#"{"type":"object","properties":{"step":{"type":"integer"}},"required":["step"],"additionalProperties":false}"#.utf8)),
    .init(name: "finish", description: "Finish", schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8), effect: .terminal),
]

actor CountingModel: AgentModelClient {
    nonisolated let usesServerSideCompaction = true
    let finishAt: Int
    var count = 0
    init(finishAt: Int) { self.finishAt = finishAt }
    func complete(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval) async throws -> AgentMessage {
        count += 1
        return .init(role: .assistant, toolCalls: [.init(id: "call-\(count)", name: count == finishAt ? "finish" : "work",
            arguments: count == finishAt ? "{}" : "{\"step\":\(count)}")])
    }
}

private actor TestMemory: AgentMemoryServicing {
    let scope: AgentMemoryScope
    var loseSyncAck: Bool
    let failWriteNumber: Int?
    var entries: [AgentMemoryEntry] = []
    var composeCalls = 0
    var writes = 0
    var reconciliations = 0
    init(scope: AgentMemoryScope, loseSyncAck: Bool = false, failWriteNumber: Int? = nil) {
        self.scope = scope; self.loseSyncAck = loseSyncAck; self.failWriteNumber = failWriteNumber
    }
    func ensureThread() {}
    func sync(_ entries: [AgentMemoryEntry], reconciling: Bool) throws {
        if reconciling {
            reconciliations += 1
            guard entries.allSatisfy({ entry in self.entries.contains { $0.id == entry.id && $0.message == entry.message } }) else { throw AgentContextError.invalidHistory }
            return
        }
        writes += 1; self.entries.append(contentsOf: entries)
        if loseSyncAck || writes == failWriteNumber {
            loseSyncAck = false; throw URLError(.networkConnectionLost)
        }
    }
    func compose() -> AgentMemoryContext {
        composeCalls += 1
        return .init(
            blocks: [],
            recentRecords: entries.map { .init(id: $0.id, message: $0.message) }
        )
    }
}
