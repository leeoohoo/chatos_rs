import ChatOSAgentRuntime
@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class LocalAgentInboxSignalTests: XCTestCase {
    func testSignalReachesInProcessModelAfterEveryToolKind() async throws {
        try await exercise(runtime: AgentRuntime())
    }

    func testSignalReachesWorkerAfterEveryToolKind() async throws {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_TEST_AGENT_WORKER_PATH"] else {
            throw XCTSkip("Set CHATOS_TEST_AGENT_WORKER_PATH for the real Worker inbox test")
        }
        try await exercise(runtime: AgentWorkerProcessClient(executableURL: URL(fileURLWithPath: path)))
    }

    private func exercise(runtime: any AgentRuntimeExecuting) async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("inbox-signal-\(UUID())")
        defer { try? FileManager.default.removeItem(at: folder) }
        let service = NativeAgentGroupChatService(databaseURL: folder.appendingPathComponent("chat.db"))
        let store = try await service.store()
        let agent = try await store.createAgent(ownerUserID: "alice", draft: .init(name: "收件箱状态", rolePrompt: "及时读取新消息", modelConfigID: "signal-model"))
        let team = try await store.createRoom(ownerUserID: "alice", projectID: "signal-team", draft: .init(name: "状态团队"))
        _ = try await store.addMember(ownerUserID: "alice", roomID: team.id, agentID: agent.id, draft: .init(role: "沟通者"))
        let direct = try await store.openHumanAgentDirect(ownerUserID: "alice", agentID: agent.id)
        _ = try await store.postMessage(ownerUserID: "alice", roomID: team.id, draft: .init(senderKind: .human, senderID: "alice", content: "开始", mentionedAgentIDs: [agent.id]), limits: .init())
        let tools = InboxSignalTestTools(store: store, directRoomID: direct.id)
        let suite = "inbox-signal-\(UUID())"
        defer { UserDefaults.standard.removePersistentDomain(forName: suite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: service, services: InboxSignalTestServices(), settings: .init(suiteName: suite), runtime: runtime,
            additionalToolProviders: { _, _, _, _ in [tools] }
        )
        let results = try await scheduler.drainCommunications(ownerUserID: "alice", maximumRuns: 1)
        XCTAssertEqual(results.count, 1)
        XCTAssertEqual(results.first?.outcome, .completed, results.first?.detail ?? "")
        let deliveryID = try XCTUnwrap(results.first?.deliveryID)
        let saved = try await store.run(ownerUserID: "alice", deliveryID: deliveryID)
        let run = try XCTUnwrap(saved)
        XCTAssertEqual(run.checkpoint.modelCalls, 10)
        XCTAssertFalse(run.events.contains { $0.kind == "context_paused" })
        XCTAssertEqual(run.checkpoint.receipts["arrive-1"]?.content, #"{"original":"unchanged"}"#)
        XCTAssertEqual(run.checkpoint.receipts["large-5"]?.content.count, 140_000)
        XCTAssertEqual(run.checkpoint.receipts["bad-schema"]?.isError, true)
        XCTAssertEqual(run.checkpoint.receipts["failed-tool"]?.isError, true)
        let observedTools = run.checkpoint.messages.indices.filter { run.checkpoint.messages[$0].role == .tool }
        XCTAssertEqual(observedTools.count, 15)
        for index in observedTools {
            let next = try XCTUnwrap(run.checkpoint.messages.dropFirst(index + 1).first)
            XCTAssertEqual(next.role, .system)
            XCTAssertTrue(next.content.hasPrefix(AgentRuntime.runtimeStateMarker), "Every tool observation must carry the inbox snapshot")
        }
        let unread = try await store.listAllUnreadMessages(ownerUserID: "alice", agentID: agent.id)
        XCTAssertTrue(unread.isEmpty)
        // Messages consumed inside this run must not trigger another empty/duplicate manager.
        let redundant = try await scheduler.drainCommunications(ownerUserID: "alice", maximumRuns: 10)
        XCTAssertTrue(redundant.isEmpty)
    }

    func testInboxPagingResetsSignalWithoutAcknowledgingLaterMessages() async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("inbox-pages-\(UUID())")
        defer { try? FileManager.default.removeItem(at: folder) }
        let service = NativeAgentGroupChatService(databaseURL: folder.appendingPathComponent("chat.db"))
        let store = try await service.store()
        let agent = try await store.createAgent(ownerUserID: "alice", draft: .init(name: "分页", rolePrompt: "读取", modelConfigID: "model"))
        let direct = try await store.openHumanAgentDirect(ownerUserID: "alice", agentID: agent.id)
        for index in 0..<3 {
            _ = try await store.postMessage(ownerUserID: "alice", roomID: direct.id, draft: .init(senderKind: .human, senderID: "alice", content: "初始消息\(index)"), limits: .init())
        }
        let claimed = try await store.claimNextDelivery(ownerUserID: "alice", agentID: agent.id, nowUnixMs: 100)
        let delivery = try XCTUnwrap(claimed)
        let context = try LocalAgentChatRunContext(ownerUserID: "alice", projectID: direct.projectID, roomID: direct.id, agentID: agent.id, deliveryID: delivery.id, triggerMessageID: delivery.messageID, rootMessageID: delivery.rootMessageID, runID: "pages", hopCount: 0)
        let provider = try await LocalAgentRelayMCPServer(service: service).connect(context: context)
        var seen = Set<String>()
        for index in 0..<3 {
            let page = try await provider.execute(.init(id: "page-\(index)", name: LocalAgentChatToolProvider.readAllUnreadToolName, arguments: #"{"limit":1}"#))
            let json = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(page.content.utf8)) as? [String: Any])
            XCTAssertEqual(json["message_count"] as? Int, 1)
            XCTAssertTrue(seen.insert(page.content).inserted, "paging must advance past this run's observed messages")
            let receipt = await provider.managerInboxSession.completionReceipt()
            let hasNew = try await store.hasNewInboxMessages(ownerUserID: "alice", agentID: agent.id, readThrough: receipt.readThrough)
            XCTAssertEqual(hasNew, index < 2)
        }
        let beforeAck = try await store.listAllUnreadMessages(ownerUserID: "alice", agentID: agent.id)
        XCTAssertEqual(beforeAck.flatMap(\.messages).count, 3)
        let late = try await store.postMessage(ownerUserID: "alice", roomID: direct.id, draft: .init(senderKind: .human, senderID: "alice", content: "尚未读取的后到消息"), limits: .init())
        let receipt = await provider.managerInboxSession.completionReceipt()
        let hasLate = try await store.hasNewInboxMessages(ownerUserID: "alice", agentID: agent.id, readThrough: receipt.readThrough)
        XCTAssertTrue(hasLate)
        let foreign = try await store.hasNewInboxMessages(ownerUserID: "bob", agentID: agent.id, readThrough: [:])
        XCTAssertFalse(foreign)
        let completed = try await provider.execute(.init(id: "finish-pages", name: LocalAgentChatToolProvider.completeManagerCycleToolName, arguments: "{}"))
        XCTAssertFalse(completed.isError, completed.content)
        let unreadAfter = try await store.listAllUnreadMessages(ownerUserID: "alice", agentID: agent.id)
        XCTAssertEqual(unreadAfter.flatMap(\.messages).map(\.id), [late.message.id])
        let lateDelivery = try await store.delivery(ownerUserID: "alice", deliveryID: XCTUnwrap(late.deliveries.first).id)
        XCTAssertEqual(lateDelivery?.status, .pending)
    }
}

extension SQLiteAgentGroupChatStoreTests {
    func testInboxSignalFailureKeepsCompletedToolReceiptAndReportsUnknown() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let runID = UUID()
        let scope = try AgentMemoryScope(tenantID: "alice", todoID: "signal", agentID: "agent", projectID: "team", runID: runID, runtimeScope: "signal-failure")
        let base = AgentMemoryContextProvider(scope: scope, service: SchedulerTestMemory())
        let context = try LocalAgentChatRunContext(ownerUserID: "alice", projectID: "team", roomID: "room", agentID: "agent", deliveryID: "delivery", triggerMessageID: "message", rootMessageID: "message", runID: runID.uuidString, hopCount: 0)
        let provider = LocalAgentManagerContextProvider(base: base, store: store, context: context, inboxSession: .init())
        var checkpoint = AgentRunCheckpoint(scope: scope.runtimeScope, messages: [.init(role: .system, content: "system"), .init(role: .user, content: "start")])
        checkpoint.id = runID
        checkpoint = try base.bind(checkpoint)
        let call = AgentToolCall(id: "saved-write", name: "write", arguments: "{}")
        let receipt = AgentToolOutcome(#"{"saved":true}"#)
        checkpoint.messages += [.init(role: .assistant, toolCalls: [call]), .init(role: .tool, content: receipt.content, toolCallID: call.id)]
        checkpoint.receipts[call.id] = receipt
        checkpoint.status = .running
        // A real SQLite query failure, confined to this test's temporary database.
        try executeSQLite(url, sql: "DROP TABLE project_agent_read_cursors")
        let capability = try AgentModelCapabilitySnapshot(revision: "test", contextWindowTokens: 128_000, outputReserveTokens: 10_000, protocolOverheadEstimatorID: "responses-json-v1", tokenizerID: nil)
        let prepared = try await provider.prepare(checkpoint: checkpoint, tools: [], modelCapability: capability, deadline: Date().addingTimeInterval(10), synchronizeOnly: true, shouldPause: { false }, record: { _, _ in })
        XCTAssertEqual(prepared.checkpoint.status, .running)
        XCTAssertNil(prepared.checkpoint.inFlightCallID)
        XCTAssertEqual(prepared.checkpoint.receipts[call.id], receipt)
        XCTAssertEqual(prepared.checkpoint.messages.filter { $0.role == .tool }, checkpoint.messages.filter { $0.role == .tool })
        XCTAssertTrue(prepared.checkpoint.messages.last?.content.contains(#""status":"unavailable","has_new_messages":null"#) == true)
        let repeated = try await provider.prepare(checkpoint: prepared.checkpoint, tools: [], modelCapability: capability, deadline: Date().addingTimeInterval(10), synchronizeOnly: true, shouldPause: { false }, record: { _, _ in })
        XCTAssertEqual(repeated.checkpoint.messages, prepared.checkpoint.messages)
    }

    func testInboxCompletionOnlyFoldsObservedUnstartedMessageWakes() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await store.createAgent(ownerUserID: "alice", draft: .init(name: "唤醒边界", rolePrompt: "读取", modelConfigID: "model"))
        let direct = try await store.openHumanAgentDirect(ownerUserID: "alice", agentID: agent.id)
        var deliveries: [ProjectAgentDelivery] = []
        for index in 0..<6 {
            let post = try await store.postMessage(ownerUserID: "alice", roomID: direct.id, draft: .init(senderKind: .human, senderID: "alice", content: "消息\(index)"), limits: .init())
            deliveries.append(try XCTUnwrap(post.deliveries.first))
        }
        let claimed = try await store.claimNextDelivery(ownerUserID: "alice", agentID: agent.id, nowUnixMs: 100)
        let current = try XCTUnwrap(claimed)
        deliveries.swapAt(0, try XCTUnwrap(deliveries.firstIndex { $0.id == current.id }))
        let runID = UUID()
        let savedDelivery = deliveries[2]
        let context = try LocalAgentChatRunContext(ownerUserID: "alice", projectID: direct.projectID, roomID: direct.id, agentID: agent.id, deliveryID: savedDelivery.id, triggerMessageID: savedDelivery.messageID, rootMessageID: savedDelivery.rootMessageID, runID: runID.uuidString.lowercased(), hopCount: 0)
        var checkpoint = AgentRunCheckpoint(scope: LocalAgentGroupChatRun.runtimeScope(for: context), messages: [.init(role: .system, content: "saved"), .init(role: .user, content: "resume")])
        checkpoint.id = runID
        checkpoint.status = .paused
        let saved = try LocalAgentGroupChatRun(id: runID, context: context, modelConfigID: "model", policy: .init(), checkpoint: checkpoint, createdAtUnixMs: 100, updatedAtUnixMs: 100)
        try await store.saveRun(saved)
        for (index, kind) in [(3, "heartbeat"), (4, "todo_status"), (5, "todo")] {
            try executeSQLite(url, sql: "UPDATE project_agent_deliveries SET trigger_kind = '\(kind)' WHERE id = '\(deliveries[index].id)'")
        }
        let unread = try await store.listAllUnreadMessages(ownerUserID: "alice", agentID: agent.id)
        let boundary = try XCTUnwrap(unread.first?.messages.last?.id)
        let late = try await store.postMessage(ownerUserID: "alice", roomID: direct.id, draft: .init(senderKind: .human, senderID: "alice", content: "后到未读"), limits: .init())
        _ = try await store.completeManagerCycleDelivery(ownerUserID: "alice", deliveryID: current.id, agentID: agent.id, readThroughMessageIDsByRoom: [direct.id: boundary], nowUnixMs: 101)
        for (index, delivery) in deliveries.enumerated() {
            let updated = try await store.delivery(ownerUserID: "alice", deliveryID: delivery.id)
            XCTAssertEqual(updated?.status, index < 2 ? .completed : .pending)
        }
        let savedAfter = try await store.run(ownerUserID: "alice", deliveryID: savedDelivery.id)
        XCTAssertEqual(savedAfter, saved)
        let lateAfter = try await store.delivery(ownerUserID: "alice", deliveryID: XCTUnwrap(late.deliveries.first).id)
        XCTAssertEqual(lateAfter?.status, .pending)
    }
}

private struct InboxSignalTestServices: AgentServiceProviding {
    private let registry = SchedulerTestMemoryRegistry()
    func makeAgentModel(configID: String, policy: AgentRunPolicy) async throws -> any AgentModelClient { InboxSignalTestModel() }
    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing { await registry.memory(for: scope) }
}

private struct InboxSignalTestTools: AgentToolProvider {
    let store: SQLiteAgentGroupChatStore
    let directRoomID: String
    func definitions() async throws -> [AgentToolDefinition] {
        [.init(name: "signal_work", description: "Verify inbox notifications", schema: Data(#"{"type":"object","properties":{"action":{"type":"string"}},"required":["action"],"additionalProperties":false}"#.utf8))]
    }
    func execute(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let args = try JSONSerialization.jsonObject(with: Data(call.arguments.utf8)) as? [String: String]
        switch args?["action"] {
        case "arrive":
            _ = try await store.postMessage(ownerUserID: "alice", roomID: directRoomID, draft: .init(senderKind: .human, senderID: "alice", content: "工具执行中到达：\(call.id)"), limits: .init())
            return .init(#"{"original":"unchanged"}"#)
        case "fail": return .failure("工具明确返回失败")
        case "large": return .init(String(repeating: "x", count: 140_000))
        default: return .failure("unexpected test action")
        }
    }
}

private actor InboxSignalTestModel: MemoryCapableTestModel {
    private var step = 0
    nonisolated var memoryCapabilitySnapshot: AgentModelCapabilitySnapshot? {
        try? .init(revision: "inbox-test", contextWindowTokens: 2_000_000, outputReserveTokens: 30_000, protocolOverheadEstimatorID: "responses-json-v1", tokenizerID: nil)
    }
    func complete(messages: [AgentMessage], tools: [AgentToolDefinition], timeout: TimeInterval) async throws -> AgentMessage {
        defer { step += 1 }
        if step > 0 {
            let state = try XCTUnwrap(messages.last { $0.role == .system && $0.content.hasPrefix(AgentRuntime.runtimeStateMarker) })
            let line = try XCTUnwrap(state.content.split(separator: "\n").dropFirst().first)
            let json = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any])
            let inbox = try XCTUnwrap(json["inbox"] as? [String: Any])
            XCTAssertEqual(inbox["has_new_messages"] as? Bool, [2, 3, 8].contains(step))
            XCTAssertEqual(inbox["read_tool"] as? String, LocalAgentChatToolProvider.readAllUnreadToolName)
        }
        func call(_ id: String, _ name: String, _ args: String = "{}") -> AgentMessage {
            .init(role: .assistant, toolCalls: [.init(id: id, name: name, arguments: args)])
        }
        switch step {
        case 0, 3: return call("read-\(step)", LocalAgentChatToolProvider.readAllUnreadToolName, #"{"limit":1}"#)
        case 1: return call("arrive-1", "signal_work", #"{"action":"arrive"}"#)
        case 2: return call("bad-schema", "signal_work", #"{"action":123}"#)
        case 4: return call("failed-tool", "signal_work", #"{"action":"fail"}"#)
        case 5:
            return .init(role: .assistant, toolCalls: (0..<6).map { .init(id: "large-\($0)", name: "signal_work", arguments: #"{"action":"large"}"#) })
        case 6:
            XCTAssertTrue(messages.last { $0.role == .tool }?.content.contains("tool_result_reference") == true)
            return call("page-large", "tool_result_read", #"{"source_call_id":"large-5","limit":50}"#)
        case 7:
            XCTAssertTrue(messages.last { $0.role == .tool }?.content.contains("next_offset") == true)
            return call("arrive-2", "signal_work", #"{"action":"arrive"}"#)
        case 8: return call("read-final", LocalAgentChatToolProvider.readAllUnreadToolName, #"{"limit":1}"#)
        default: return call("finish", LocalAgentChatToolProvider.completeManagerCycleToolName)
        }
    }
}
