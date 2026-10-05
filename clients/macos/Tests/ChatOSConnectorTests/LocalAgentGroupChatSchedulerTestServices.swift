import ChatOSAgentRuntime
@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

enum SchedulerTestError: Error {
    case memoryOffline
}
struct MemoryOfflineSchedulerServices: AgentServiceProviding {
    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy
    ) async throws -> any AgentModelClient {
        XCTFail("Memory 未连接时不应调用模型")
        return SchedulerTestModel()
    }

    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        SchedulerOfflineMemory()
    }
}

struct SchedulerOfflineMemory: AgentMemoryServicing {
    func ensureThread() async throws { throw SchedulerTestError.memoryOffline }
    func sync(_ entries: [AgentMemoryEntry], reconciling: Bool) async throws {
        throw SchedulerTestError.memoryOffline
    }
    func compose() async throws -> AgentMemoryContext {
        throw SchedulerTestError.memoryOffline
    }
}

struct SchedulerTestServices: AgentServiceProviding {
    var expectedThinkingLevel: String?
    private let memoryRegistry: SchedulerTestMemoryRegistry

    init(expectedThinkingLevel: String? = nil) {
        self.expectedThinkingLevel = expectedThinkingLevel
        self.memoryRegistry = SchedulerTestMemoryRegistry()
    }

    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy
    ) async throws -> any AgentModelClient {
        XCTAssertEqual(configID, "local-model")
        return SchedulerTestModel()
    }

    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy,
        thinkingLevel: String?
    ) async throws -> any AgentModelClient {
        XCTAssertEqual(configID, "local-model")
        XCTAssertEqual(thinkingLevel, expectedThinkingLevel)
        return SchedulerTestModel()
    }

    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        await memoryRegistry.memory(for: scope)
    }
}

/// The production Memory service reconnects to the same durable thread when a Run resumes.
/// Keep that contract in scheduler tests while isolating concurrently executing Runs.
actor SchedulerTestMemoryRegistry {
    private var memoriesByRunID: [UUID: SchedulerTestMemory] = [:]

    func memory(for scope: AgentMemoryScope) -> SchedulerTestMemory {
        if let memory = memoriesByRunID[scope.runID] {
            return memory
        }
        let memory = SchedulerTestMemory()
        memoriesByRunID[scope.runID] = memory
        return memory
    }
}

actor SchedulerTestMemory: AgentMemoryServicing {
    private var records: [Int: AgentMemoryContextRecord] = [:]

    func ensureThread() async throws {}

    func sync(_ entries: [AgentMemoryEntry], reconciling: Bool) async throws {
        for entry in entries {
            records[entry.index] = .init(id: entry.id, message: entry.message)
        }
    }

    func compose() async throws -> AgentMemoryContext {
        .init(
            blocks: [],
            recentRecords: records.keys.sorted().compactMap { records[$0] }
        )
    }
}

struct TodoRetrySchedulerTestServices: AgentServiceProviding {
    let model: TodoRetrySchedulerTestModel
    private let memoryRegistry = SchedulerTestMemoryRegistry()

    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy
    ) async throws -> any AgentModelClient {
        XCTAssertEqual(configID, "todo-retry-model")
        return model
    }

    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        await memoryRegistry.memory(for: scope)
    }
}

actor TodoRetrySchedulerTestModel: AgentModelClient {
    private var requestCount = 0

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        requestCount += 1
        if requestCount == 1 {
            throw AgentRuntimeError.invalidResponse
        }
        XCTAssertTrue(tools.contains {
            $0.name == LocalAgentChatToolProvider.todoCompleteToolName
        })
        return .init(
            role: .assistant,
            toolCalls: [.init(
                id: "complete-retried-todo",
                name: LocalAgentChatToolProvider.todoCompleteToolName,
                arguments: #"{"summary":"恢复原 Run 后完成。"}"#
            )]
        )
    }
}

actor SchedulerTestModel: AgentModelClient {
    private var requestCount = 0

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        XCTAssertTrue(messages.contains {
            $0.role == .system && $0.content.contains("chatos-capability-discovery")
        })
        XCTAssertTrue(messages.contains {
            $0.role == .user && $0.content.contains("读取全部未读")
        })
        XCTAssertFalse(messages.contains {
            $0.role == .user && $0.content.contains("current_trigger_json")
        })
        requestCount += 1
        switch requestCount {
        case 1:
            return .init(
                role: .assistant,
                toolCalls: [.init(
                    id: "send-reply",
                    name: LocalAgentChatToolProvider.sendMessageToolName,
                    arguments: #"{"content":"我已在客户端完成本地调度。"}"#
                )]
            )
        case 2:
            return .init(
                role: .assistant,
                toolCalls: [.init(
                    id: "read-schedule",
                    name: LocalAgentChatToolProvider.todoScheduleStateToolName,
                    arguments: "{}"
                )]
            )
        default:
            return .init(
                role: .assistant,
                toolCalls: [.init(
                    id: "complete-cycle",
                    name: LocalAgentChatToolProvider.completeManagerCycleToolName,
                    arguments: "{}"
                )]
            )
        }
    }
}

struct ParallelSchedulerTestServices: AgentServiceProviding {
    let probe: SchedulerConcurrencyProbe
    private let memoryRegistry = SchedulerTestMemoryRegistry()

    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy
    ) async throws -> any AgentModelClient {
        XCTAssertEqual(configID, "parallel-model")
        return ParallelSchedulerTestModel(probe: probe)
    }

    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing {
        await memoryRegistry.memory(for: scope)
    }
}

actor SchedulerConcurrencyProbe {
    private var activeCalls = 0
    private var maximumCalls = 0

    func enter() {
        activeCalls += 1
        maximumCalls = max(maximumCalls, activeCalls)
    }

    func leave() {
        activeCalls -= 1
    }

    func maximumConcurrentCalls() -> Int {
        maximumCalls
    }
}

actor ParallelSchedulerTestModel: AgentModelClient {
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
        try await Task.sleep(for: .milliseconds(50))
        await probe.leave()
        requestCount += 1
        if requestCount == 1 {
            return .init(
                role: .assistant,
                toolCalls: [.init(id: "read-unread", name: "chat_read_unread", arguments: "{}")]
            )
        }
        if requestCount == 2 {
            return .init(
                role: .assistant,
                toolCalls: [.init(
                    id: "send-reply",
                    name: LocalAgentChatToolProvider.sendMessageToolName,
                    arguments: #"{"content":"并行 Agent 已完成。"}"#
                )]
            )
        }
        return .init(
            role: .assistant,
            toolCalls: [.init(
                id: "complete-cycle",
                name: LocalAgentChatToolProvider.completeManagerCycleToolName,
                arguments: "{}"
            )]
        )
    }
}
