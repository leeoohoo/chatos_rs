import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class LocalAgentGroupChatSchedulerTests: XCTestCase {
    func testSchedulerRunsDeliveryLocallyAndPersistsCompletedRun() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-scheduler-\(UUID().uuidString)")
        let databaseURL = folder.appendingPathComponent("chat.db")
        defer { try? FileManager.default.removeItem(at: folder) }

        let nativeService = NativeAgentGroupChatService(databaseURL: databaseURL)
        let store = try await nativeService.store()
        let agent = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "客户端工程师",
                description: "实现客户端功能",
                rolePrompt: "先读取群聊，再给出可执行答复。",
                modelConfigID: "local-model"
            )
        )
        let room = try await store.createRoom(
            ownerUserID: "alice",
            projectID: "project-1",
            draft: .init(name: "项目群", goal: "完成本地多 Agent 协作")
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "客户端工程师", responsibility: "实现群聊")
        )
        _ = try await store.setDefaultAgent(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id
        )
        let post = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(senderKind: .human, senderID: "alice", content: "开始实现"),
            limits: .init()
        )
        let delivery = try XCTUnwrap(post.deliveries.first)

        let settingsSuite = "local-agent-scheduler-tests-\(UUID().uuidString)"
        defer { UserDefaults.standard.removePersistentDomain(forName: settingsSuite) }
        let scheduler = LocalAgentGroupChatScheduler(
            service: nativeService,
            services: SchedulerTestServices(),
            settings: .init(suiteName: settingsSuite)
        )
        let results = try await scheduler.drainProject(
            ownerUserID: "alice",
            projectID: "project-1"
        )

        XCTAssertEqual(results.count, 1)
        XCTAssertEqual(results.first?.outcome, .completed)
        let completedDelivery = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: delivery.id
        )
        XCTAssertEqual(completedDelivery?.status, .completed)
        let messages = try await store.listMessages(
            ownerUserID: "alice",
            roomID: room.id,
            afterUnixMs: nil,
            limit: 10
        )
        XCTAssertEqual(messages.map(\.content), ["开始实现", "我已在客户端完成本地调度。"])

        let savedRun = try await store.run(ownerUserID: "alice", deliveryID: delivery.id)
        XCTAssertEqual(savedRun?.checkpoint.status, .completed)
        XCTAssertEqual(savedRun?.context.agentID, agent.id)
        XCTAssertEqual(savedRun?.context.projectID, "project-1")
        XCTAssertNil(savedRun?.checkpoint.memory)
        XCTAssertTrue(savedRun?.events.contains(where: { $0.kind == "memory_unavailable" }) == true)
    }
}

private enum SchedulerTestError: Error {
    case memoryOffline
}

private struct SchedulerTestServices: AgentServiceProviding {
    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy
    ) async throws -> any AgentModelClient {
        XCTAssertEqual(configID, "local-model")
        return SchedulerTestModel()
    }

    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        throw SchedulerTestError.memoryOffline
    }
}

private actor SchedulerTestModel: AgentModelClient {
    private var requestCount = 0

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        requestCount += 1
        switch requestCount {
        case 1:
            return .init(
                role: .assistant,
                toolCalls: [.init(id: "read-trigger", name: "chat_get_trigger", arguments: "{}")]
            )
        default:
            return .init(
                role: .assistant,
                toolCalls: [.init(
                    id: "send-reply",
                    name: "chat_send_message",
                    arguments: #"{"content":"我已在客户端完成本地调度。"}"#
                )]
            )
        }
    }
}
