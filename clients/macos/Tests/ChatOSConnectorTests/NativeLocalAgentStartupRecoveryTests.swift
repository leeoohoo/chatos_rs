// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSConnector
import ChatOSCore
import Testing

@Suite("Native Local Agent startup recovery")
struct NativeLocalAgentStartupRecoveryTests {
    @Test("restarts the whole projection restore on a replacement Host endpoint")
    func retriesOnReplacementHost() async throws {
        let failed = StartupRecoveryClient(error: NativeLocalAgentIPCError.connectionClosed)
        let replacement = StartupRecoveryClient(error: nil)
        let host = StartupRecoveryHost(clients: [failed, replacement])
        let conversations = ConversationHistoryStore()
        let tasks = LocalAgentTaskStateStore()
        let recovery = NativeLocalAgentStartupRecovery(
            clientProvider: { try await host.client() },
            stateProvider: { await host.state() },
            mainChatStore: conversations,
            taskStore: tasks,
            retryDelay: .zero
        )

        _ = try await recovery.restore()

        let turn = try #require(
            await conversations.snapshot(sessionID: "thread-1").turns.first
        )
        #expect(turn.id == "turn-1")
        #expect(turn.finalAssistantMessage?.text == "Recovered result")
        #expect(await host.clientRequestCount() == 2)
        #expect(await failed.runListRequestCount() > 0)
        #expect(await replacement.runListRequestCount() == 2)
    }

    @Test("fails closed on protocol data errors instead of retrying another Host")
    func rejectsNonTransientFailure() async throws {
        let invalid = StartupRecoveryClient(error: NativeLocalAgentIPCError.invalidResponse)
        let unused = StartupRecoveryClient(error: nil)
        let host = StartupRecoveryHost(clients: [invalid, unused])
        let recovery = NativeLocalAgentStartupRecovery(
            clientProvider: { try await host.client() },
            stateProvider: { await host.state() },
            mainChatStore: ConversationHistoryStore(),
            taskStore: LocalAgentTaskStateStore(),
            retryDelay: .zero
        )

        await #expect(throws: NativeLocalAgentIPCError.invalidResponse) {
            _ = try await recovery.restore()
        }
        #expect(await host.clientRequestCount() == 1)
    }
}

private actor StartupRecoveryHost {
    private let clients: [StartupRecoveryClient]
    private var requestCount = 0

    init(clients: [StartupRecoveryClient]) {
        self.clients = clients
    }

    func state() -> NativeLocalAgentHostState {
        let index = min(requestCount, clients.count - 1)
        return .running(
            accountID: "user-1",
            processID: UInt32(index + 1),
            clientEndpoint: "/tmp/agent-\(index).sock",
            restartCount: index
        )
    }

    func client() throws -> any NativeLocalAgentStartupRecoveryClient {
        guard requestCount < clients.count else {
            throw NativeLocalAgentAccountSessionError.hostUnavailable
        }
        let client = clients[requestCount]
        requestCount += 1
        return client
    }

    func clientRequestCount() -> Int { requestCount }
}

private actor StartupRecoveryClient: NativeLocalAgentStartupRecoveryClient {
    private let error: NativeLocalAgentIPCError?
    private var runListRequests = 0

    init(error: NativeLocalAgentIPCError?) {
        self.error = error
    }

    func runs(cursor: String?, limit: UInt32) async throws -> (
        runs: [LocalAgentRunSnapshot], nextCursor: String?
    ) {
        runListRequests += 1
        if let error { throw error }
        return ([mainRun], nil)
    }

    func tasks(cursor: String?, limit: UInt32) async throws -> (
        tasks: [LocalAgentTaskSnapshot], nextCursor: String?
    ) {
        if let error { throw error }
        return ([], nil)
    }

    func run(id: String) async throws -> LocalAgentRunSnapshot {
        if let error { throw error }
        return try #require(id == mainRun.runID ? mainRun : nil)
    }

    func task(id: String) async throws -> LocalAgentTaskSnapshot {
        throw NativeLocalAgentIPCError.invalidResponse
    }

    func runDetail(
        id: String,
        eventLimit: UInt32,
        eventOffset: UInt32
    ) async throws -> LocalAgentRunDetail {
        if let error { throw error }
        return LocalAgentRunDetail(
            run: mainRun,
            events: [],
            tools: [],
            eventsTotal: 0,
            eventsHasMore: false,
            snapshotEventSequence: 5
        )
    }

    func mainChatRunBinding(runID: String) async throws -> LocalAgentMainChatRunBinding {
        if let error { throw error }
        return LocalAgentMainChatRunBinding(
            runID: runID,
            threadID: "thread-1",
            turnID: "turn-1",
            messageID: "message-1",
            userMessage: LocalAgentStoredMessage(
                recordID: "message-1",
                runID: runID,
                threadID: "thread-1",
                turnID: "turn-1",
                sequence: 1,
                role: .user,
                content: "Restore this design",
                messageMode: .semantic,
                messageSource: "main_chat",
                memorySyncStatus: .synced,
                createdAt: "2026-09-13T01:00:00Z"
            )
        )
    }

    func runListRequestCount() -> Int { runListRequests }

    private var mainRun: LocalAgentRunSnapshot {
        LocalAgentRunSnapshot(
            runID: "main-run-1",
            profileKey: "main_chat",
            ownerUserID: "user-1",
            ownerEntityType: "conversation",
            ownerEntityID: "thread-1",
            projectID: "project-1",
            status: .succeeded,
            version: 2,
            stepSeq: 2,
            iteration: 1,
            retryCount: 0,
            modelConfigID: "model-1",
            modelConfigRevision: 1,
            modelRuntimeSnapshot: .object([:]),
            contextStrategy: "provider_native",
            promptRevision: "prompt-1",
            capabilitySnapshotRef: "capabilities-1",
            terminalOutcome: .object(["text": .string("Recovered result")]),
            createdAt: "2026-09-13T01:00:00Z",
            updatedAt: "2026-09-13T01:01:00Z"
        )
    }
}
