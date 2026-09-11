import Foundation
import XCTest
@testable import ChatOSAgentRuntime

final class AgentMemoryContextTests: XCTestCase {
    func test600CallsKeepModelInputBoundedAndAuditHistoryComplete() async throws {
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
        XCTAssertGreaterThan(result.memory?.compactions ?? 0, 1)
        let largest = await model.largestInput
        XCTAssertLessThanOrEqual(largest, contextPolicy.hardInputLimit)
        let stored = await memory.entries.count
        XCTAssertEqual(stored, result.messages.count)
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

    func testNoOpSummaryPausesWithoutSendingOversizedInput() async throws {
        var (checkpoint, scope) = try fixture()
        checkpoint.messages[1].content = String(repeating: "故事", count: 2_000)
        let memory = TestMemory(scope: scope, noImprovement: true)
        let provider = AgentMemoryContextProvider(scope: scope, service: memory)
        let model = CountingModel(finishAt: 1)
        var policy = AgentRunPolicy(); policy.context = contextPolicy
        let result = try await AgentRuntime().run(checkpoint: provider.bind(checkpoint), scope: checkpoint.scope,
            policy: policy, model: model, tools: tools, execute: { _ in .init("ok") }, contextProvider: provider)
        XCTAssertEqual(result.status, .paused)
        XCTAssertEqual(result.modelCalls, 0)
        XCTAssertEqual(result.memory?.syncedMessageCount, 2)
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
        XCTAssertEqual(writes, 2, "Initial batch and final response, not a resend of the initial batch")
    }

    func testResumesExistingSummaryJobWithoutSubmittingAgain() async throws {
        let (initial, scope) = try fixture()
        let memory = TestMemory(scope: scope, summaryPollsBeforeCompletion: 1)
        let provider = AgentMemoryContextProvider(scope: scope, service: memory)
        var checkpoint = try provider.bind(initial)
        for index in 0..<20 { checkpoint.messages.append(.init(role: .assistant, content: "\(index)" + String(repeating: "x", count: 250))) }
        let synced = try await provider.prepare(checkpoint: checkpoint, tools: tools, policy: contextPolicy,
            deadline: Date().addingTimeInterval(60), synchronizeOnly: true, record: { _, _ in })
        checkpoint = synced.checkpoint
        var resumePolicy = contextPolicy
        resumePolicy.summaryPollSeconds = 1
        let result = try await provider.prepare(checkpoint: checkpoint, tools: tools, policy: resumePolicy,
            deadline: Date().addingTimeInterval(60), record: { _, _ in })
        XCTAssertGreaterThan(result.checkpoint.memory?.compactions ?? 0, 0)
        let starts = await memory.summaryStarts
        XCTAssertEqual(starts, 0)
    }

    func testTerminalCompletionSurvivesFinalSyncFailureWithoutAnotherModelOrToolCall() async throws {
        let (checkpoint, scope) = try fixture()
        let memory = TestMemory(scope: scope, failWriteNumber: 2)
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

    func testCancellationWhileWaitingPreservesSummaryJobForResume() async throws {
        let (initial, scope) = try fixture()
        let memory = TestMemory(scope: scope, waitsForSummary: true)
        let provider = AgentMemoryContextProvider(scope: scope, service: memory)
        var checkpoint = try provider.bind(initial)
        for index in 0..<20 { checkpoint.messages.append(.init(role: .assistant, content: "\(index)" + String(repeating: "x", count: 250))) }
        var policy = contextPolicy
        policy.summaryPollSeconds = 1
        policy.summaryTimeoutSeconds = 5
        let definitions = tools
        let preparedCheckpoint = checkpoint
        let task = Task {
            try await provider.prepare(checkpoint: preparedCheckpoint, tools: definitions, policy: policy,
                                       deadline: Date().addingTimeInterval(10), record: { _, _ in })
        }
        try await Task.sleep(for: .milliseconds(100))
        task.cancel()
        do {
            _ = try await task.value
            XCTFail("Should pause while waiting")
        } catch let failure as AgentContextPreparationFailure {
            XCTAssertTrue(failure.cancelled)
            checkpoint = failure.checkpoint
        }
        await memory.finishSummary()
        let restored = try JSONDecoder().decode(AgentRunCheckpoint.self, from: JSONEncoder().encode(checkpoint))
        _ = try await provider.prepare(checkpoint: restored, tools: tools, policy: contextPolicy,
            deadline: Date().addingTimeInterval(60), record: { _, _ in })
        let starts = await memory.summaryStarts
        XCTAssertEqual(starts, 0, "Continue the server-owned job, not a second summary")
    }

    func testDefaultSummaryPollingSleepCompletesWithoutCrashingRuntime() async throws {
        var (checkpoint, scope) = try fixture()
        let memory = TestMemory(scope: scope, summaryPollsBeforeCompletion: 1)
        let provider = AgentMemoryContextProvider(scope: scope, service: memory)
        checkpoint = try provider.bind(checkpoint)
        for index in 0..<20 {
            checkpoint.messages.append(.init(role: .assistant, content: "\(index)" + String(repeating: "x", count: 250)))
        }
        var policy = contextPolicy
        policy.summaryPollSeconds = 1
        policy.summaryTimeoutSeconds = 5
        let result = try await provider.prepare(checkpoint: checkpoint, tools: tools, policy: policy,
                                                deadline: Date().addingTimeInterval(10), record: { _, _ in })
        XCTAssertGreaterThan(result.checkpoint.memory?.compactions ?? 0, 0)
        let polls = await memory.summaryStatusPolls
        XCTAssertEqual(polls, 2, "One Task Runner-style in-flight check plus one job poll")
    }

    func testCancellingDefaultSummaryPollingSleepDoesNotCrashRuntime() async throws {
        var (checkpoint, scope) = try fixture()
        let memory = TestMemory(scope: scope, waitsForSummary: true)
        let provider = AgentMemoryContextProvider(scope: scope, service: memory)
        checkpoint = try provider.bind(checkpoint)
        for index in 0..<20 {
            checkpoint.messages.append(.init(role: .assistant, content: "\(index)" + String(repeating: "x", count: 250)))
        }
        var policy = contextPolicy
        policy.summaryPollSeconds = 1
        policy.summaryTimeoutSeconds = 5
        let definitions = tools
        let preparedCheckpoint = checkpoint
        let task = Task {
            try await provider.prepare(checkpoint: preparedCheckpoint, tools: definitions, policy: policy,
                                       deadline: Date().addingTimeInterval(10), record: { _, _ in })
        }
        try await Task.sleep(for: .milliseconds(100))
        task.cancel()
        do {
            _ = try await task.value
            XCTFail("Cancelled polling should stop context preparation")
        } catch let failure as AgentContextPreparationFailure {
            XCTAssertTrue(failure.cancelled)
            XCTAssertEqual(failure.checkpoint.memory?.syncedMessageCount, checkpoint.messages.count)
        }
    }

    private var contextPolicy: AgentContextPolicy {
        var policy = AgentContextPolicy(); policy.windowTokens = 2_048; policy.outputReserveTokens = 512
        policy.compactionThresholdTokens = 900
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
    let finishAt: Int
    var count = 0
    var largestInput = 0
    init(finishAt: Int) { self.finishAt = finishAt }
    func complete(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval) async throws -> AgentMessage {
        largestInput = max(largestInput, try AgentContextBudget.estimate(messages: messages, tools: tools))
        count += 1
        return .init(role: .assistant, toolCalls: [.init(id: "call-\(count)", name: count == finishAt ? "finish" : "work",
            arguments: count == finishAt ? "{}" : "{\"step\":\(count)}")])
    }
}

private actor TestMemory: AgentMemoryServicing {
    let scope: AgentMemoryScope
    let noImprovement: Bool
    var loseSyncAck: Bool
    let failWriteNumber: Int?
    var waitsForSummary: Bool
    var entries: [AgentMemoryEntry] = []
    var retainedFrom = 0
    var summaryStarts = 0
    var summaryStatusPolls = 0
    var summaryPollsBeforeCompletion: Int
    var writes = 0
    var reconciliations = 0
    init(scope: AgentMemoryScope, noImprovement: Bool = false, loseSyncAck: Bool = false, failWriteNumber: Int? = nil,
         waitsForSummary: Bool = false, summaryPollsBeforeCompletion: Int = 0) {
        self.scope = scope; self.noImprovement = noImprovement; self.loseSyncAck = loseSyncAck; self.failWriteNumber = failWriteNumber
        self.waitsForSummary = waitsForSummary; self.summaryPollsBeforeCompletion = summaryPollsBeforeCompletion
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
        .init(blocks: retainedFrom > 0
                ? [.init(blockType: "thread_summary", text: "Previous work is saved. Continue the remaining steps.")]
                : [],
              recentRecords: entries.dropFirst(retainedFrom).map { .init(id: $0.id, message: $0.message) })
    }
    func finishSummary() { waitsForSummary = false; if !noImprovement { retainedFrom = max(0, entries.count - 1) } }
    func startSummary(reason: String) -> AgentSummaryStatus {
        summaryStarts += 1
        if waitsForSummary || summaryPollsBeforeCompletion > 0 { return .init(jobID: "existing", running: true) }
        finishSummary()
        return .init(jobID: "existing", completed: true, compacted: !noImprovement)
    }
    func summaryStatus(jobID: String?) -> AgentSummaryStatus {
        summaryStatusPolls += 1
        if jobID == nil {
            if waitsForSummary || summaryPollsBeforeCompletion > 0 {
                return .init(jobID: "existing", running: true)
            }
            if summaryStarts == 0 { return .init() }
        }
        if jobID != nil && summaryPollsBeforeCompletion > 0 {
            summaryPollsBeforeCompletion -= 1
            if summaryPollsBeforeCompletion == 0 { finishSummary() }
        }
        let running = waitsForSummary || summaryPollsBeforeCompletion > 0
        return .init(jobID: "existing", running: running, completed: !running, compacted: !running && !noImprovement)
    }
}
