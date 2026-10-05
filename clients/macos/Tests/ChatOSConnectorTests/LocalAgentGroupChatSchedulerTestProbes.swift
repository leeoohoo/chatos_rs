import ChatOSAgentRuntime
@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

struct LaneSchedulerTestServices: AgentServiceProviding {
    let probe: SchedulerConcurrencyProbe
    private let memoryRegistry = SchedulerTestMemoryRegistry()

    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy
    ) async throws -> any AgentModelClient {
        XCTAssertEqual(configID, "lane-model")
        return LaneSchedulerTestModel(probe: probe)
    }

    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        await memoryRegistry.memory(for: scope)
    }
}

struct CommunicationFastLaneServices: AgentServiceProviding {
    let probe: CommunicationFastLaneProbe
    private let memoryRegistry = SchedulerTestMemoryRegistry()

    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy
    ) async throws -> any AgentModelClient {
        XCTAssertEqual(configID, "fast-lane-model")
        return CommunicationFastLaneModel(probe: probe)
    }

    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        await memoryRegistry.memory(for: scope)
    }
}

actor CommunicationFastLaneProbe {
    private let holdsFirstManagerCall: Bool
    private var executorStarted = false
    private var executorReleased = false
    private var executorCallCount = 0
    private var managerStarted = false
    private var managerReleased = false
    private var managerCallCount = 0
    private var startWaiters: [CheckedContinuation<Void, Never>] = []
    private var releaseWaiters: [CheckedContinuation<Void, Never>] = []
    private var managerStartWaiters: [CheckedContinuation<Void, Never>] = []
    private var managerReleaseWaiters: [CheckedContinuation<Void, Never>] = []

    init(holdsFirstManagerCall: Bool = false) {
        self.holdsFirstManagerCall = holdsFirstManagerCall
    }

    func markExecutorStarted() {
        executorCallCount += 1
        executorStarted = true
        let waiters = startWaiters
        startWaiters.removeAll()
        for waiter in waiters { waiter.resume() }
    }

    func waitUntilExecutorStarts() async {
        guard !executorStarted else { return }
        await withCheckedContinuation { startWaiters.append($0) }
    }

    func waitUntilExecutorReleased() async {
        guard !executorReleased else { return }
        await withCheckedContinuation { releaseWaiters.append($0) }
    }

    func releaseExecutor() {
        executorReleased = true
        let waiters = releaseWaiters
        releaseWaiters.removeAll()
        for waiter in waiters { waiter.resume() }
    }

    func executorCalls() -> Int {
        executorCallCount
    }

    func beginManagerCall() async {
        managerCallCount += 1
        guard holdsFirstManagerCall, managerCallCount == 1 else { return }
        managerStarted = true
        let startWaiters = managerStartWaiters
        managerStartWaiters.removeAll()
        for waiter in startWaiters { waiter.resume() }
        guard !managerReleased else { return }
        await withCheckedContinuation { managerReleaseWaiters.append($0) }
    }

    func waitUntilManagerStarts() async {
        guard !managerStarted else { return }
        await withCheckedContinuation { managerStartWaiters.append($0) }
    }

    func releaseManager() {
        managerReleased = true
        let waiters = managerReleaseWaiters
        managerReleaseWaiters.removeAll()
        for waiter in waiters { waiter.resume() }
    }

    func managerCalls() -> Int {
        managerCallCount
    }
}

actor CommunicationFastLaneModel: AgentModelClient {
    private let probe: CommunicationFastLaneProbe
    private var managerRequestCount = 0

    init(probe: CommunicationFastLaneProbe) {
        self.probe = probe
    }

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        if tools.contains(where: { $0.name == LocalAgentChatToolProvider.todoCompleteToolName }) {
            await probe.markExecutorStarted()
            await probe.waitUntilExecutorReleased()
            return .init(role: .assistant, toolCalls: [.init(
                id: "complete-long-executor",
                name: LocalAgentChatToolProvider.todoCompleteToolName,
                arguments: #"{"summary":"长时间任务完成。"}"#
            )])
        }

        managerRequestCount += 1
        await probe.beginManagerCall()
        if managerRequestCount == 1 {
            return .init(role: .assistant, toolCalls: [.init(
                id: "reply-during-execution",
                name: LocalAgentChatToolProvider.sendMessageToolName,
                arguments: #"{"content":"执行任务期间也已及时回复。"}"#
            )])
        }
        return .init(role: .assistant, toolCalls: [.init(
            id: "complete-fast-manager-cycle",
            name: LocalAgentChatToolProvider.completeManagerCycleToolName,
            arguments: "{}"
        )])
    }
}

actor LaneSchedulerTestModel: AgentModelClient {
    private let probe: SchedulerConcurrencyProbe
    private var requestCount = 0

    init(probe: SchedulerConcurrencyProbe) {
        self.probe = probe
    }

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        await probe.enter()
        try await Task.sleep(for: .milliseconds(75))
        await probe.leave()
        if tools.contains(where: { $0.name == LocalAgentChatToolProvider.todoCompleteToolName }) {
            return .init(
                role: .assistant,
                toolCalls: [.init(
                    id: "complete-todo",
                    name: LocalAgentChatToolProvider.todoCompleteToolName,
                    arguments: #"{"summary":"执行线程已完成。"}"#
                )]
            )
        }
        requestCount += 1
        if requestCount > 1 {
            return .init(
                role: .assistant,
                toolCalls: [.init(
                    id: "complete-manager-cycle",
                    name: LocalAgentChatToolProvider.completeManagerCycleToolName,
                    arguments: "{}"
                )]
            )
        }
        return .init(
            role: .assistant,
            toolCalls: [.init(
                id: "reply-manager",
                name: LocalAgentChatToolProvider.sendMessageToolName,
                arguments: #"{"content":"通讯线程已回复。"}"#
            )]
        )
    }
}

struct CancellationSchedulerTestServices: AgentServiceProviding {
    let store: SQLiteAgentGroupChatStore
    let agentID: String
    let todoID: String
    private let memoryRegistry = SchedulerTestMemoryRegistry()

    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy
    ) async throws -> any AgentModelClient {
        XCTAssertEqual(configID, "cancellation-model")
        return CancellationSchedulerTestModel(store: store, agentID: agentID, todoID: todoID)
    }

    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        await memoryRegistry.memory(for: scope)
    }
}

actor CancellationSchedulerTestModel: AgentModelClient {
    let store: SQLiteAgentGroupChatStore
    let agentID: String
    let todoID: String

    init(store: SQLiteAgentGroupChatStore, agentID: String, todoID: String) {
        self.store = store
        self.agentID = agentID
        self.todoID = todoID
    }

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: agentID,
            todoID: todoID,
            update: .init(status: .cancelled),
            nowUnixMs: 201
        )
        return .init(
            role: .assistant,
            toolCalls: [.init(
                id: "rejected-progress",
                name: LocalAgentChatToolProvider.todoProgressAppendToolName,
                arguments: #"{"detail":"不应产生的副作用"}"#
            )]
        )
    }
}

struct ActiveCancellationSchedulerTestServices: AgentServiceProviding {
    private let memoryRegistry = SchedulerTestMemoryRegistry()

    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy
    ) async throws -> any AgentModelClient {
        XCTAssertEqual(configID, "active-cancellation-model")
        return ActiveCancellationSchedulerTestModel()
    }

    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        await memoryRegistry.memory(for: scope)
    }
}

actor ActiveCancellationSchedulerTestModel: AgentModelClient {
    private var requestCount = 0

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        let isExecutorLane = messages.contains {
            $0.role == .user && $0.content.contains("执行当前绑定的 Todo")
        }
        if isExecutorLane {
            try await Task.sleep(for: .seconds(30))
            return .init(role: .assistant, content: "不应自然结束")
        }
        requestCount += 1
        switch requestCount {
        case 1:
            return .init(role: .assistant, toolCalls: [.init(
                id: "activate-todo-planning",
                name: LocalAgentChatToolProvider.agentSkillActivateToolName,
                arguments: #"{"skill_ref":"product-skill:chatos-todo-planning"}"#
            )])
        case 2:
            return .init(role: .assistant, toolCalls: [.init(
                id: "list-todos-before-cancel",
                name: LocalAgentChatToolProvider.todoListToolName,
                arguments: "{}"
            )])
        case 3:
            let toolContent = messages.last(where: { $0.role == .tool })?.content ?? "[]"
            let data = Data(toolContent.utf8)
            let values = try JSONSerialization.jsonObject(with: data) as? [[String: Any]]
            let todoReference = try XCTUnwrap(values?.first?["todo_ref"] as? String)
            let arguments = try XCTUnwrap(String(
                data: JSONSerialization.data(withJSONObject: [
                    "todo_ref": todoReference,
                    "status": "cancelled",
                ]),
                encoding: .utf8
            ))
            return .init(role: .assistant, toolCalls: [.init(
                id: "cancel-running-todo",
                name: LocalAgentChatToolProvider.todoUpdateToolName,
                arguments: arguments
            )])
        case 4:
            return .init(role: .assistant, toolCalls: [.init(
                id: "activate-collaboration-messaging",
                name: LocalAgentChatToolProvider.agentSkillActivateToolName,
                arguments: #"{"skill_ref":"product-skill:chatos-collaboration-messaging"}"#
            )])
        default:
            return .init(role: .assistant, toolCalls: [.init(
                id: "complete-after-cancel",
                name: LocalAgentChatToolProvider.completeManagerCycleToolName,
                arguments: "{}"
            )])
        }
    }
}
