import Foundation
import XCTest
@testable import ChatOSAgentRuntime

final class AgentMemoryContextTests: XCTestCase {
    func testRuntimeProfilesCannotBecomeAgentIdentities() throws {
        for agentID in ["main-chat", "main_chat", "task-execution", "task_execution"] {
            XCTAssertThrowsError(try AgentMemoryScope(
                tenantID: "user-a",
                agentID: agentID,
                projectID: "project-1",
                runID: UUID(),
                runtimeScope: "main-chat"
            )) { error in
                guard case AgentRuntimeError.scopeMismatch = error else {
                    return XCTFail("Expected scopeMismatch, got \(error)")
                }
            }
            XCTAssertThrowsError(try AgentMemoryScope(
                tenantID: "user-a",
                todoID: "todo-a",
                agentID: agentID,
                projectID: "project-1",
                runID: UUID(),
                runtimeScope: "task-execution"
            )) { error in
                guard case AgentRuntimeError.scopeMismatch = error else {
                    return XCTFail("Expected scopeMismatch, got \(error)")
                }
            }
        }
    }

    func testLocalAgentMemoryIsStableAcrossProjectsAndPrivateToAgent() throws {
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
        let firstInAnotherProject = try AgentMemoryScope(
            tenantID: "user-a",
            agentID: "agent-a",
            projectID: "project-2",
            runID: UUID(),
            runtimeScope: "account:user-a:project:project-2:agent:agent-a"
        )
        XCTAssertEqual(first.subjectID, "agent:agent-a")
        XCTAssertEqual(first.projectSubjectID, "project:project-1")
        XCTAssertEqual(firstInAnotherProject.subjectID, first.subjectID)
        XCTAssertEqual(firstInAnotherProject.threadID, first.threadID)
        XCTAssertEqual(firstInAnotherProject.projectSubjectID, "project:project-2")
        XCTAssertNil(first.projectAggregationSubjectID)
        XCTAssertNil(firstInAnotherProject.projectAggregationSubjectID)
        XCTAssertEqual(second.subjectID, "agent:agent-b")
        XCTAssertNotEqual(first.subjectID, second.subjectID)
        XCTAssertNotEqual(first.threadID, second.threadID)
        XCTAssertEqual(first.includeSubjectMemory, true)
    }

    func testDirectMessageToolDoesNotCreateAProjectOrAnotherAgentThread() throws {
        let projectScope = try AgentMemoryScope(
            tenantID: "user-a",
            agentID: "agent-a",
            projectID: "project-1",
            runID: UUID(),
            runtimeScope: "project-communication"
        )
        let directScope = try AgentMemoryScope(
            tenantID: "user-a",
            agentID: "agent-a",
            projectID: nil,
            runID: UUID(),
            runtimeScope: "direct-message-tool"
        )

        XCTAssertEqual(directScope.threadID, projectScope.threadID)
        XCTAssertEqual(directScope.subjectID, projectScope.subjectID)
        XCTAssertNil(directScope.projectSubjectID)
        XCTAssertNil(directScope.projectAggregationSubjectID)
    }

    func testTodoMemoryIsStableForOneTodoAndIsolatedFromManagerAndOtherTodos() throws {
        let manager = try AgentMemoryScope(
            tenantID: "user-a", agentID: "agent-a", projectID: "project-1",
            runID: UUID(), runtimeScope: "manager"
        )
        let firstAttempt = try AgentMemoryScope(
            tenantID: "user-a", todoID: "todo-a", agentID: "agent-a", projectID: "project-1",
            runID: UUID(), runtimeScope: "executor-a"
        )
        let resumedAttempt = try AgentMemoryScope(
            tenantID: "user-a", todoID: "todo-a", agentID: "agent-a", projectID: "project-1",
            runID: UUID(), runtimeScope: "executor-b"
        )
        let otherTodo = try AgentMemoryScope(
            tenantID: "user-a", todoID: "todo-b", agentID: "agent-a", projectID: "project-1",
            runID: UUID(), runtimeScope: "executor-c"
        )

        XCTAssertEqual(firstAttempt.threadID, resumedAttempt.threadID)
        XCTAssertEqual(firstAttempt.subjectID, resumedAttempt.subjectID)
        XCTAssertNotEqual(firstAttempt.threadID, otherTodo.threadID)
        XCTAssertNotEqual(firstAttempt.threadID, manager.threadID)
        XCTAssertEqual(firstAttempt.subjectID, manager.subjectID)
        XCTAssertEqual(firstAttempt.projectSubjectID, manager.projectSubjectID)
        XCTAssertEqual(firstAttempt.projectAggregationSubjectID, "project:project-1")
        XCTAssertNil(manager.projectAggregationSubjectID)
        XCTAssertEqual(firstAttempt.includeSubjectMemory, true)
        XCTAssertTrue(firstAttempt.allowsCrossRunHistory)
    }

    func testApprovalAgentUsesOneTenantScopedThreadAndWaitsForThresholdSummary() async throws {
        let first = try AgentMemoryScope(
            tenantID: "user-a", profile: "approval", projectID: "workspace-1",
            runID: UUID(), runtimeScope: "approval:first"
        )
        let second = try AgentMemoryScope(
            tenantID: "user-a", profile: "approval", projectID: "workspace-2",
            runID: UUID(), runtimeScope: "approval:second"
        )

        XCTAssertEqual(first.threadID, "client-agent:agent:local_connector_command_approval_agent")
        XCTAssertEqual(second.threadID, first.threadID)
        XCTAssertEqual(first.subjectID, "agent:local_connector_command_approval_agent")
        XCTAssertEqual(second.subjectID, first.subjectID)
        XCTAssertEqual(first.projectSubjectID, "project:workspace-1")
        XCTAssertEqual(second.projectSubjectID, "project:workspace-2")
        XCTAssertNil(first.projectAggregationSubjectID)
        XCTAssertNil(second.projectAggregationSubjectID)
        XCTAssertTrue(first.allowsCrossRunHistory)

        let memory = TestMemory(scope: first)
        let provider = AgentMemoryContextProvider(scope: first, service: memory)
        try await provider.finalizeMemory()
        let flushes = await memory.flushes
        XCTAssertEqual(flushes, 0, "Approval history must accumulate until the normal summary threshold")
    }

    func testTaskThreadFlushesAndContributesToAgentAndProjectMemory() async throws {
        let scope = try AgentMemoryScope(
            tenantID: "user-a", todoID: "todo-a", agentID: "agent-a", projectID: "project-a",
            runID: UUID(), runtimeScope: "task:todo-a"
        )
        let memory = TestMemory(scope: scope)
        let provider = AgentMemoryContextProvider(scope: scope, service: memory)

        try await provider.finalizeMemory()

        XCTAssertEqual(scope.subjectID, "agent:agent-a")
        XCTAssertEqual(scope.projectAggregationSubjectID, "project:project-a")
        let flushes = await memory.flushes
        XCTAssertEqual(flushes, 1)
    }

    func testComposeStartsBackgroundCompactionAndUsesBudgetedProjection() async throws {
        let (checkpoint, scope) = try fixture()
        let memory = TestMemory(
            scope: scope,
            requiresCompaction: true,
            firstComposeBlockCharacters: 32
        )
        let provider = AgentMemoryContextProvider(scope: scope, service: memory)

        let prepared = try await provider.prepare(
            checkpoint: provider.bind(checkpoint),
            tools: tools,
            modelCapability: try modelCapability(),
            deadline: Date().addingTimeInterval(60),
            record: { _, _ in }
        )

        XCTAssertTrue(prepared.messages.contains { $0.content.contains(String(repeating: "h", count: 32)) })
        XCTAssertEqual(prepared.messages.last, checkpoint.messages.last)
        let composeCalls = await memory.composeCalls
        let flushes = await memory.flushes
        XCTAssertEqual(composeCalls, 1)
        XCTAssertEqual(flushes, 1)
    }

    func testCompactionGenerationFailureKeepsBudgetedProjectionAndCurrentTurn() async throws {
        let (checkpoint, scope) = try fixture()
        let memory = TestMemory(
            scope: scope,
            requiresCompaction: true,
            compactionFailure: true,
            firstComposeBlockCharacters: 32
        )
        let provider = AgentMemoryContextProvider(scope: scope, service: memory)

        let prepared = try await provider.prepare(
            checkpoint: provider.bind(checkpoint),
            tools: tools,
            modelCapability: try modelCapability(),
            deadline: Date().addingTimeInterval(60),
            record: { _, _ in }
        )

        XCTAssertTrue(prepared.messages.contains { $0.content.contains(String(repeating: "h", count: 32)) })
        XCTAssertEqual(prepared.messages.last, checkpoint.messages.last)
        let composeCalls = await memory.composeCalls
        let flushes = await memory.flushes
        XCTAssertEqual(composeCalls, 1)
        XCTAssertEqual(flushes, 1)
    }

    func testTransientCompactionFailureKeepsBudgetedProjectionAndCurrentTurn() async throws {
        let (checkpoint, scope) = try fixture()
        let memory = TestMemory(
            scope: scope,
            requiresCompaction: true,
            transientCompactionFailure: true,
            firstComposeBlockCharacters: 32
        )
        let provider = AgentMemoryContextProvider(scope: scope, service: memory)

        let prepared = try await provider.prepare(
            checkpoint: provider.bind(checkpoint),
            tools: tools,
            modelCapability: try modelCapability(),
            deadline: Date().addingTimeInterval(60),
            record: { _, _ in }
        )

        XCTAssertTrue(prepared.messages.contains { $0.content.contains(String(repeating: "h", count: 32)) })
        XCTAssertEqual(prepared.messages.last, checkpoint.messages.last)
        let composeCalls = await memory.composeCalls
        let flushes = await memory.flushes
        XCTAssertEqual(composeCalls, 1)
        XCTAssertEqual(flushes, 1)
    }

    func testRuntimeRecomposesMemoryBeforeStoppingForBudget() async throws {
        let (checkpoint, scope) = try fixture()
        let memory = TestMemory(scope: scope, firstComposeBlockCharacters: 10_000)
        let provider = AgentMemoryContextProvider(scope: scope, service: memory)
        let model = BudgetRecoveryModel()

        let result = try await AgentRuntime().run(
            checkpoint: provider.bind(checkpoint),
            scope: checkpoint.scope,
            policy: .init(),
            model: model,
            tools: tools,
            execute: { call in
                call.name == "finish"
                    ? .init("done")
                    : .init(String(repeating: "x", count: 6_000))
            },
            contextProvider: provider
        )

        XCTAssertEqual(result.status, .completed, result.stopReason ?? "")
        XCTAssertEqual(result.modelCalls, 2)
        let composeCalls = await memory.composeCalls
        XCTAssertEqual(composeCalls, 2)
    }

    func test600ResponsesCallsComposeMemoryOnceAndKeepAuditHistoryComplete() async throws {
        let (checkpoint, scope) = try fixture()
        let memory = TestMemory(scope: scope)
        let provider = AgentMemoryContextProvider(scope: scope, service: memory)
        let model = CountingModel(finishAt: 600)
        let policy = AgentRunPolicy()
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

    func testComposeUsesBlocksRecentRecordsAndStickyTaskLikeTaskExecution() throws {
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
        XCTAssertEqual(result[1].role, .system)
        XCTAssertTrue(result[1].content.contains("[thread_summary]\nprevious work"))
        XCTAssertTrue(result[1].content.contains("historical context only"))
        XCTAssertEqual(result.filter { $0.role == .tool }.map(\.toolCallID), ["a", "b"])
        XCTAssertEqual(result[2], checkpoint.messages[1], "Current task must precede its live continuation")
        XCTAssertEqual(result.last?.content, "next")
    }

    func testComposeAcceptsEquivalentResponseOutputWithDifferentJSONKeyOrder() throws {
        var (checkpoint, scope) = try fixture()
        let checkpointJSON = Data(#"[{"type":"reasoning","id":"item-1","encrypted_content":"secret","summary":[{"type":"summary_text","text":"thinking"}]}]"#.utf8)
        let memoryJSON = Data(#"[{"summary":[{"text":"thinking","type":"summary_text"}],"encrypted_content":"secret","id":"item-1","type":"reasoning"}]"#.utf8)
        let message = AgentMessage(
            role: .assistant,
            content: "done",
            toolCalls: [.init(id: "call-1", name: "read", arguments: "{\"path\":\"README.md\"}")],
            responseOutputJSON: checkpointJSON,
            usage: .init(inputTokens: 12, cachedTokens: 3, outputTokens: 4, requests: 1)
        )
        checkpoint.messages.append(message)
        checkpoint.messages.append(.init(role: .tool, content: "result", toolCallID: "call-1"))
        var memoryMessage = message
        memoryMessage.responseOutputJSON = memoryJSON
        let memory = AgentMemoryCheckpoint(scope: scope, pinnedMessageCount: 2)
        let context = AgentMemoryContext(
            blocks: [.init(blockType: "thread_summary", text: "summary")],
            recentRecords: [.init(id: scope.recordID(at: 2), message: memoryMessage)]
        )

        XCTAssertNoThrow(try AgentContextAssembler.assemble(
            checkpoint: checkpoint, memory: memory, context: context
        ))
    }

    func testComposeStillRejectsRealResponseMessageChangesAndMalformedJSON() throws {
        var (checkpoint, scope) = try fixture()
        let responseJSON = Data(#"[{"type":"reasoning","id":"item-1","encrypted_content":"secret"}]"#.utf8)
        let message = AgentMessage(
            role: .assistant,
            content: "done",
            toolCalls: [.init(id: "call-1", name: "read", arguments: "{}")],
            responseOutputJSON: responseJSON,
            usage: .init(inputTokens: 12, outputTokens: 4, requests: 1)
        )
        checkpoint.messages.append(message)
        checkpoint.messages.append(.init(role: .tool, content: "result", toolCallID: "call-1"))
        let memory = AgentMemoryCheckpoint(scope: scope, pinnedMessageCount: 2)

        var changedContent = message
        changedContent.content = "changed"
        var changedCall = message
        changedCall.toolCalls[0].name = "write"
        var changedUsage = message
        changedUsage.usage?.outputTokens = 5
        var changedEncryptedContent = message
        changedEncryptedContent.responseOutputJSON = Data(#"[{"type":"reasoning","id":"item-1","encrypted_content":"different"}]"#.utf8)
        var missingResponseOutput = message
        missingResponseOutput.responseOutputJSON = nil
        var malformedResponseOutput = message
        malformedResponseOutput.responseOutputJSON = Data("not-json".utf8)

        for changed in [changedContent, changedCall, changedUsage, changedEncryptedContent,
                        missingResponseOutput, malformedResponseOutput] {
            let context = AgentMemoryContext(
                blocks: [.init(blockType: "thread_summary", text: "summary")],
                recentRecords: [.init(id: scope.recordID(at: 2), message: changed)]
            )
            XCTAssertThrowsError(try AgentContextAssembler.assemble(
                checkpoint: checkpoint, memory: memory, context: context
            )) { error in
                XCTAssertEqual(error as? AgentContextError, .invalidHistory)
            }
        }
    }

    func testStableAgentThreadKeepsEarlierRawRecordsAndCurrentProjectSummary() throws {
        let earlierRunID = UUID()
        let currentRunID = UUID()
        let earlierScope = try AgentMemoryScope(
            tenantID: "user-a", agentID: "agent-a", projectID: "project-1",
            runID: earlierRunID, runtimeScope: "account:user-a:project:project-1:agent:agent-a"
        )
        let currentScope = try AgentMemoryScope(
            tenantID: "user-a", agentID: "agent-a", projectID: "project-2",
            runID: currentRunID, runtimeScope: "account:user-a:project:project-2:agent:agent-a"
        )
        XCTAssertEqual(earlierScope.threadID, currentScope.threadID)

        let earlierReply = AgentMessage(role: .assistant, content: "已经创建三国自走棋团队")
        let currentSystem = AgentMessage(role: .system, content: "Only authorized tools")
        let currentRequest = AgentMessage(role: .user, content: "那你帮我处理好吧")
        var checkpoint = AgentRunCheckpoint(
            scope: currentScope.runtimeScope,
            messages: [currentSystem, currentRequest]
        )
        checkpoint.id = currentRunID
        let memory = AgentMemoryCheckpoint(scope: currentScope, pinnedMessageCount: 2)
        let context = AgentMemoryContext(
            blocks: [.init(blockType: "project_summary_recent", text: "当前项目状态")],
            recentRecords: [
                .init(id: earlierScope.recordID(at: 2), message: earlierReply),
                .init(id: currentScope.recordID(at: 0), message: currentSystem),
                .init(id: currentScope.recordID(at: 1), message: currentRequest),
            ]
        )

        let result = try AgentContextAssembler.assemble(
            checkpoint: checkpoint, memory: memory, context: context
        )
        XCTAssertTrue(result.contains { $0.role == .system && $0.content.contains("project_summary_recent") })
        XCTAssertTrue(result.contains(earlierReply))
        XCTAssertEqual(result.last, currentRequest)
    }

    func testCrossRunHistoryStillRejectsMalformedDuplicateAndUnknownCurrentRunRecords() throws {
        let runID = UUID()
        let scope = try AgentMemoryScope(
            tenantID: "user-a", agentID: "agent-a", projectID: "project-1",
            runID: runID, runtimeScope: "account:user-a:project:project-1:agent:agent-a"
        )
        var checkpoint = AgentRunCheckpoint(
            scope: scope.runtimeScope,
            messages: [.init(role: .system, content: "system"), .init(role: .user, content: "request")]
        )
        checkpoint.id = runID
        let memory = AgentMemoryCheckpoint(scope: scope, pinnedMessageCount: 2)
        let earlierID = "client-agent:\(UUID().uuidString):message:0"
        let earlierRecord = AgentMemoryContextRecord(
            id: earlierID, message: .init(role: .assistant, content: "history")
        )
        let invalidContexts: [AgentMemoryContext] = [
            .init(blocks: [], recentRecords: [
                .init(id: "another-run-record", message: earlierRecord.message),
            ]),
            .init(blocks: [], recentRecords: [earlierRecord, earlierRecord]),
            .init(blocks: [], recentRecords: [
                .init(id: "client-agent:\(runID.uuidString):message:99", message: earlierRecord.message),
            ]),
        ]

        for context in invalidContexts {
            XCTAssertThrowsError(try AgentContextAssembler.assemble(
                checkpoint: checkpoint, memory: memory, context: context
            ))
        }
    }

    func testRejectsMissingHistoryWithoutSummaryAndUnknownRecords() throws {
        var (checkpoint, scope) = try fixture()
        checkpoint.messages += [.init(role: .assistant, content: "must not be dropped"),
                                .init(role: .user, content: "next turn")]
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
            var checkpoint = base
            checkpoint.messages += [message, .init(role: .user, content: "next turn")]
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
        let policy = AgentRunPolicy()
        let result = try await AgentRuntime().run(checkpoint: provider.bind(checkpoint), scope: checkpoint.scope,
            policy: policy, model: model, tools: tools, execute: { _ in .init("ok") }, contextProvider: provider)
        XCTAssertEqual(result.status, .completed)
        XCTAssertEqual(result.modelCalls, 1)
        XCTAssertEqual(result.memory?.syncedMessageCount, result.messages.count)
    }

    func testFixedCurrentTurnBeyondSoftBudgetSkipsRecallAndUsesResponsesCompaction() async throws {
        var (checkpoint, scope) = try fixture()
        checkpoint.messages[1].content = String(repeating: "x", count: 50_000)
        let memory = TestMemory(scope: scope)
        let provider = AgentMemoryContextProvider(scope: scope, service: memory)
        let model = FixedTurnCompactionModel()

        let result = try await AgentRuntime().run(
            checkpoint: provider.bind(checkpoint),
            scope: checkpoint.scope,
            policy: .init(),
            model: model,
            tools: tools,
            execute: { _ in .init("done") },
            contextProvider: provider
        )

        XCTAssertEqual(result.status, .completed, result.stopReason ?? "")
        XCTAssertEqual(result.modelCalls, 1)
        let composeCalls = await memory.composeCalls
        XCTAssertEqual(composeCalls, 0, "A zero/negative recall budget must not be sent to Memory Engine")
        let modelCalls = await model.count
        XCTAssertEqual(modelCalls, 1)
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
        checkpoint = try await provider.prepare(checkpoint: checkpoint, tools: tools,
            modelCapability: try modelCapability(),
            deadline: Date().addingTimeInterval(60), synchronizeOnly: true, record: { _, _ in }).checkpoint
        checkpoint.messages[1].content = "mutated"
        do {
            _ = try await provider.prepare(checkpoint: checkpoint, tools: tools,
                modelCapability: try modelCapability(),
                deadline: Date().addingTimeInterval(60), record: { _, _ in })
            XCTFail("Should fail")
        } catch { XCTAssertTrue(error is AgentContextPreparationFailure) }
        var other = initial; other.scope = "another-account"
        XCTAssertThrowsError(try provider.bind(other))
        let writes = await memory.writes
        XCTAssertEqual(writes, 1)
    }

    func testResumeAdoptsCurrentPublishedCapacityAndKeepsExecutionHistory() async throws {
        for hasOldPolicy in [true, false] {
            let (initial, scope) = try fixture()
            let memory = TestMemory(scope: scope)
            let provider = AgentMemoryContextProvider(scope: scope, service: memory)
            let model = CountingModel(finishAt: 1)
            let current = try XCTUnwrap(model.memoryCapabilitySnapshot)
            var checkpoint = try provider.bind(initial)
            checkpoint.status = .paused
            checkpoint.modelCalls = 1
            if hasOldPolicy {
                checkpoint.memory?.modelCapability = try AgentModelCapabilitySnapshot(
                    revision: "retired-capacity-v1",
                    contextWindowTokens: current.contextWindowTokens - 1000,
                    outputReserveTokens: current.outputReserveTokens,
                    protocolOverheadEstimatorID: current.protocolOverheadEstimatorID,
                    tokenizerID: current.tokenizerID
                )
            }
            let recovered = try await AgentRuntime().run(
                checkpoint: checkpoint,
                scope: checkpoint.scope,
                policy: .init(),
                model: model,
                tools: tools,
                execute: { _ in .init("done") },
                contextProvider: provider
            )
            XCTAssertEqual(recovered.id, checkpoint.id)
            XCTAssertEqual(recovered.status, .completed, recovered.stopReason ?? "")
            XCTAssertEqual(recovered.memory?.modelCapability, current)
            XCTAssertGreaterThan(recovered.modelCalls, checkpoint.modelCalls)
            XCTAssertEqual(Array(recovered.messages.prefix(checkpoint.messages.count)), checkpoint.messages)
        }
    }

    private func modelCapability() throws -> AgentModelCapabilitySnapshot {
        try AgentModelCapabilitySnapshot(
            revision: "test-capability-v1",
            contextWindowTokens: 2_048,
            outputReserveTokens: 512,
            protocolOverheadEstimatorID: "responses-json-v1",
            tokenizerID: nil
        )
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
    nonisolated let memoryCapabilitySnapshot = try? AgentModelCapabilitySnapshot(
        revision: "test-capability-v1",
        contextWindowTokens: 200_000,
        outputReserveTokens: 30_000,
        protocolOverheadEstimatorID: "responses-json-v1",
        tokenizerID: nil
    )
    let finishAt: Int
    var count = 0
    init(finishAt: Int) { self.finishAt = finishAt }
    func complete(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval) async throws -> AgentMessage {
        count += 1
        return .init(role: .assistant, toolCalls: [.init(id: "call-\(count)", name: count == finishAt ? "finish" : "work",
            arguments: count == finishAt ? "{}" : "{\"step\":\(count)}")])
    }
}

private actor BudgetRecoveryModel: AgentModelClient {
    nonisolated let usesServerSideCompaction = true
    nonisolated let memoryCapabilitySnapshot = try? AgentModelCapabilitySnapshot(
        revision: "budget-recovery-v1",
        contextWindowTokens: 4_096,
        outputReserveTokens: 512,
        protocolOverheadEstimatorID: "responses-json-v1",
        tokenizerID: nil
    )
    var count = 0
    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        count += 1
        let finish = count == 2
        return .init(role: .assistant, toolCalls: [
            .init(
                id: "budget-call-\(count)",
                name: finish ? "finish" : "work",
                arguments: finish ? "{}" : "{\"step\":1}"
            ),
        ])
    }
}

private actor FixedTurnCompactionModel: AgentModelClient {
    nonisolated let usesServerSideCompaction = true
    nonisolated let memoryCapabilitySnapshot = try? AgentModelCapabilitySnapshot(
        revision: "fixed-turn-compaction-v1",
        contextWindowTokens: 20_000,
        outputReserveTokens: 10_000,
        protocolOverheadEstimatorID: "responses-json-v1",
        tokenizerID: nil
    )
    private(set) var count = 0

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        count += 1
        return .init(role: .assistant, toolCalls: [
            .init(id: "fixed-turn-finish", name: "finish", arguments: "{}"),
        ])
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
    var flushes = 0
    let requiresCompaction: Bool
    let compactionFailure: Bool
    let transientCompactionFailure: Bool
    let firstComposeBlockCharacters: Int
    init(
        scope: AgentMemoryScope,
        loseSyncAck: Bool = false,
        failWriteNumber: Int? = nil,
        requiresCompaction: Bool = false,
        compactionFailure: Bool = false,
        transientCompactionFailure: Bool = false,
        firstComposeBlockCharacters: Int = 0
    ) {
        self.scope = scope
        self.loseSyncAck = loseSyncAck
        self.failWriteNumber = failWriteNumber
        self.requiresCompaction = requiresCompaction
        self.compactionFailure = compactionFailure
        self.transientCompactionFailure = transientCompactionFailure
        self.firstComposeBlockCharacters = firstComposeBlockCharacters
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
    func compose(_ request: AgentMemoryComposeRequest) -> AgentMemoryContext {
        composeCalls += 1
        return .init(
            blocks: composeCalls == 1 && firstComposeBlockCharacters > 0
                ? [.init(blockType: "history", text: String(repeating: "h", count: firstComposeBlockCharacters))]
                : [],
            recentRecords: entries.filter { $0.conversationTurnID != request.currentTurnID }
                .map { .init(id: $0.id, message: $0.message) },
            needsCompaction: requiresCompaction && flushes == 0,
            budgetUsedTokens: requiresCompaction && flushes == 0 ? 1_000 : 100,
            pendingRecordCount: requiresCompaction && flushes == 0 ? 10 : 1
        )
    }
    func flushSummary() throws {
        flushes += 1
        if compactionFailure {
            throw AgentMemoryCompactionFailure(reason: "stream closed before response.completed")
        }
        if transientCompactionFailure { throw URLError(.timedOut) }
    }
}
