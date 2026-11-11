import ChatOSAgentRuntime
@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

extension LocalAgentGroupChatSchedulerTests {
    func testResumePreservesSyncedRouterWhenCurrentCatalogHasChanged() async throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("frozen-router-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let service = NativeAgentGroupChatService(databaseURL: folder.appendingPathComponent("chat.db"))
        let store = try await service.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice", draft: .init(name: "router-test", rolePrompt: "Keep history", modelConfigID: "local-model")
        )
        let room = try await store.openHumanAgentDirect(ownerUserID: "alice", agentID: agent.id)
        let post = try await store.postMessage(
            ownerUserID: "alice", roomID: room.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "continue"), limits: .init()
        )
        let claimed = try await store.claimNextDelivery(ownerUserID: "alice", agentID: agent.id, nowUnixMs: post.message.createdAtUnixMs + 1)
        let delivery = try XCTUnwrap(claimed)
        let runID = UUID()
        let context = try LocalAgentChatRunContext(
            ownerUserID: "alice", projectID: room.projectID, roomID: room.id, agentID: agent.id,
            deliveryID: delivery.id, triggerMessageID: post.message.id, rootMessageID: post.message.rootMessageID,
            runID: runID.uuidString.lowercased(), hopCount: delivery.hopCount
        )
        var checkpoint = AgentRunCheckpoint(scope: LocalAgentGroupChatRun.runtimeScope(for: context), messages: [
            .init(role: .system, content: "system"), .init(role: .user, content: "读取全部未读并完成通讯周期"),
        ])
        checkpoint.id = runID
        let scope = try AgentMemoryScope(
            tenantID: "alice", agentID: agent.id, projectID: nil, runID: runID, runtimeScope: checkpoint.scope
        )
        let services = SchedulerTestServices()
        let memory = try await services.makeAgentMemory(scope: scope)
        let provider = AgentMemoryContextProvider(scope: scope, service: memory)
        checkpoint = try provider.bind(checkpoint)
        checkpoint.messages.append(.init(
            role: .system, content: "<!-- chatos-product-skill-router -->\nFrozen earlier catalog"
        ))
        checkpoint = try await provider.prepare(
            checkpoint: checkpoint, tools: [], policy: .init(),
            modelCapability: XCTUnwrap(SchedulerTestModel().memoryCapabilitySnapshot),
            deadline: Date().addingTimeInterval(30), synchronizeOnly: true, record: { _, _ in }
        ).checkpoint
        checkpoint.status = .paused
        let frozenMessages = checkpoint.messages
        let run = try LocalAgentGroupChatRun(
            id: runID, context: context, modelConfigID: "local-model", policy: .init(), checkpoint: checkpoint,
            createdAtUnixMs: post.message.createdAtUnixMs + 1, updatedAtUnixMs: post.message.createdAtUnixMs + 1
        )
        try await store.saveRun(run)
        let suite = "frozen-router-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: suite) }
        let scheduler = LocalAgentGroupChatScheduler(service: service, services: services, settings: .init(suiteName: suite))

        let result = try await scheduler.resumeDelivery(ownerUserID: "alice", projectID: room.projectID, deliveryID: delivery.id)

        XCTAssertEqual(result.outcome, .completed)
        let loaded = try await store.run(ownerUserID: "alice", deliveryID: delivery.id)
        let completed = try XCTUnwrap(loaded)
        XCTAssertEqual(Array(completed.checkpoint.messages.prefix(frozenMessages.count)), frozenMessages)
        XCTAssertEqual(completed.checkpoint.memory?.syncedMessageCount, completed.checkpoint.messages.count)
        XCTAssertFalse(completed.events.contains { $0.kind == "context_paused" })
    }
}
