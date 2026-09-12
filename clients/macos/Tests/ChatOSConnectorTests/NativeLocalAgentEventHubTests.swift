// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSConnector
import ChatOSCore
import Foundation
import Testing

@Suite("Native local Agent account event hub")
struct NativeLocalAgentEventHubTests {
    @Test("routes Main Chat events once and acknowledges only after the whole page applies")
    func routesAndAcknowledges() async throws {
        let run = mainChatRun()
        let events = [
            LocalAgentUIEvent(
                eventSeq: 1,
                emittedAt: timestamp,
                event: .runSnapshot(run)
            ),
            LocalAgentUIEvent(
                eventSeq: 2,
                emittedAt: timestamp,
                event: .modelStream(LocalAgentModelStreamEvent(
                    runID: run.runID,
                    stepSeq: 1,
                    deltaKind: .content,
                    delta: "Hello"
                ))
            ),
            LocalAgentUIEvent(
                eventSeq: 3,
                emittedAt: timestamp,
                event: .memorySync(LocalAgentMemorySyncStatus(
                    runID: run.runID,
                    pendingCount: 1,
                    failedCount: 0
                ))
            ),
            LocalAgentUIEvent(
                eventSeq: 4,
                emittedAt: timestamp,
                event: .hostStatus(LocalAgentHostRuntimeStatus(
                    state: .ready,
                    activeRunCount: 1
                ))
            ),
        ]
        let client = EventClient(events: events, runs: [run.runID: run])
        let sink = EventSink()
        let hub = NativeLocalAgentEventHub(clientProvider: { client }, sink: sink)

        let result = try await hub.drainAvailableEvents()

        #expect(result == NativeLocalAgentEventDrainResult(
            initialSequence: 0,
            acknowledgedSequence: 4,
            appliedEventCount: 4
        ))
        #expect(await client.acknowledgements() == [4])
        #expect(await client.bindingRequestCount() == 1)
        #expect(await client.runRequestCount() == 0)
        let applied = await sink.appliedEvents()
        #expect(applied.map(\.sequence) == [1, 2, 3, 4])
        #expect(applied[0].threadID == "thread-1")
        #expect(applied[1].threadID == "thread-1")
        #expect(applied[2].threadID == "thread-1")
        #expect(applied[3].threadID == nil)
    }

    @Test("does not advance the durable cursor when applying one event fails")
    func replaysUnacknowledgedPage() async throws {
        let events = [hostEvent(sequence: 1), hostEvent(sequence: 2)]
        let client = EventClient(events: events)
        let sink = EventSink(failOnceAt: 2)
        let hub = NativeLocalAgentEventHub(clientProvider: { client }, sink: sink)

        await #expect(throws: TestEventError.applyFailed) {
            _ = try await hub.drainAvailableEvents()
        }
        #expect(await client.acknowledgements().isEmpty)
        #expect(await client.currentCursor() == 0)

        let replay = try await hub.drainAvailableEvents()
        #expect(replay.appliedEventCount == 2)
        #expect(replay.acknowledgedSequence == 2)
        #expect(await client.acknowledgements() == [2])
        #expect(await sink.appliedEvents().map(\.sequence) == [1, 2, 1, 2])
    }

    @Test("rejects an unordered Host page before applying or acknowledging it")
    func rejectsUnorderedPage() async throws {
        let client = EventClient(events: [hostEvent(sequence: 2), hostEvent(sequence: 1)])
        let sink = EventSink()
        let hub = NativeLocalAgentEventHub(clientProvider: { client }, sink: sink)

        await #expect(throws: NativeLocalAgentEventHubError.self) {
            _ = try await hub.drainAvailableEvents()
        }
        #expect(await sink.appliedEvents().isEmpty)
        #expect(await client.acknowledgements().isEmpty)
    }

    @Test("resolves the replacement IPC client after a supervised Host restart")
    func followsRestartedHostEndpoint() async throws {
        let first = EventClient(events: [hostEvent(sequence: 1)])
        let replacement = EventClient(events: [hostEvent(sequence: 2)], cursor: 1)
        let provider = EventClientProvider(client: first)
        let sink = EventSink()
        let hub = NativeLocalAgentEventHub(
            clientProvider: { await provider.client() },
            sink: sink
        )

        let beforeRestart = try await hub.drainAvailableEvents()
        await provider.install(replacement)
        let afterRestart = try await hub.drainAvailableEvents()

        #expect(beforeRestart.acknowledgedSequence == 1)
        #expect(afterRestart.initialSequence == 1)
        #expect(afterRestart.acknowledgedSequence == 2)
        #expect(await first.acknowledgements() == [1])
        #expect(await replacement.acknowledgements() == [2])
        #expect(await provider.requestCount() == 2)
        #expect(await sink.appliedEvents().map(\.sequence) == [1, 2])
    }
}

private let timestamp = "2026-09-12T04:00:00Z"

private func mainChatRun() -> LocalAgentRunSnapshot {
    LocalAgentRunSnapshot(
        runID: "run-1",
        profileKey: "main_chat",
        ownerUserID: "user-1",
        ownerEntityType: "conversation",
        ownerEntityID: "thread-1",
        projectID: "project-1",
        status: .modelRunning,
        version: 2,
        stepSeq: 1,
        iteration: 0,
        retryCount: 0,
        modelConfigID: "model-1",
        modelConfigRevision: 1,
        modelRuntimeSnapshot: .object([:]),
        contextStrategy: "provider_native",
        promptRevision: "prompt-1",
        capabilitySnapshotRef: "capabilities-1",
        createdAt: timestamp,
        updatedAt: timestamp
    )
}

private func hostEvent(sequence: UInt64) -> LocalAgentUIEvent {
    LocalAgentUIEvent(
        eventSeq: sequence,
        emittedAt: timestamp,
        event: .hostStatus(LocalAgentHostRuntimeStatus(
            state: .ready,
            activeRunCount: 0
        ))
    )
}

private enum TestEventError: Error {
    case missingRun
    case applyFailed
}

private actor EventClient: NativeLocalAgentEventClient {
    private let allEvents: [LocalAgentUIEvent]
    private let runsByID: [String: LocalAgentRunSnapshot]
    private var cursor: UInt64 = 0
    private var acknowledged: [UInt64] = []
    private var bindingRequests = 0
    private var runRequests = 0

    init(
        events: [LocalAgentUIEvent],
        runs: [String: LocalAgentRunSnapshot] = [:],
        cursor: UInt64 = 0
    ) {
        self.allEvents = events
        self.runsByID = runs
        self.cursor = cursor
    }

    func run(id: String) async throws -> LocalAgentRunSnapshot {
        runRequests += 1
        guard let run = runsByID[id] else { throw TestEventError.missingRun }
        return run
    }

    func mainChatRunBinding(runID: String) async throws -> LocalAgentMainChatRunBinding {
        bindingRequests += 1
        guard let run = runsByID[runID] else { throw TestEventError.missingRun }
        return LocalAgentMainChatRunBinding(
            runID: runID,
            threadID: run.ownerEntityID,
            turnID: "turn-1",
            messageID: "message-1",
            userMessage: LocalAgentStoredMessage(
                recordID: "message-1",
                runID: runID,
                threadID: run.ownerEntityID,
                turnID: "turn-1",
                sequence: 1,
                role: .user,
                content: "Design it",
                messageMode: .semantic,
                messageSource: "main_chat",
                memorySyncStatus: .pending,
                createdAt: timestamp
            )
        )
    }

    func events(after sequence: UInt64, limit: UInt32) async throws -> (
        events: [LocalAgentUIEvent], nextSequence: UInt64, hasMore: Bool
    ) {
        let remaining = allEvents.filter { $0.eventSeq > sequence }
        let page = Array(remaining.prefix(Int(limit)))
        return (
            page,
            page.last?.eventSeq ?? sequence,
            remaining.count > page.count
        )
    }

    func uiEventCursor() async throws -> UInt64 { cursor }

    func acknowledgeUIEvents(through sequence: UInt64) async throws -> UInt64 {
        acknowledged.append(sequence)
        cursor = sequence
        return cursor
    }

    func acknowledgements() -> [UInt64] { acknowledged }
    func currentCursor() -> UInt64 { cursor }
    func bindingRequestCount() -> Int { bindingRequests }
    func runRequestCount() -> Int { runRequests }
}

private actor EventClientProvider {
    private var current: EventClient
    private var requests = 0

    init(client: EventClient) {
        current = client
    }

    func client() -> any NativeLocalAgentEventClient {
        requests += 1
        return current
    }

    func install(_ client: EventClient) {
        current = client
    }

    func requestCount() -> Int { requests }
}

private actor EventSink: LocalAgentUIEventApplying {
    struct Applied: Sendable {
        var sequence: UInt64
        var threadID: String?
    }

    private var applied: [Applied] = []
    private let failOnceAt: UInt64?
    private var hasFailed = false

    init(failOnceAt: UInt64? = nil) {
        self.failOnceAt = failOnceAt
    }

    func applyLocalAgentUIEvent(
        _ event: LocalAgentUIEvent,
        mainChatBinding: LocalAgentMainChatRunBinding?
    ) async throws {
        applied.append(Applied(
            sequence: event.eventSeq,
            threadID: mainChatBinding?.threadID
        ))
        if event.eventSeq == failOnceAt, !hasFailed {
            hasFailed = true
            throw TestEventError.applyFailed
        }
    }

    func appliedEvents() -> [Applied] { applied }
}
