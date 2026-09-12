// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

public protocol NativeLocalAgentEventClient: Sendable {
    func run(id: String) async throws -> LocalAgentRunSnapshot
    func mainChatRunBinding(runID: String) async throws -> LocalAgentMainChatRunBinding
    func events(after sequence: UInt64, limit: UInt32) async throws -> (
        events: [LocalAgentUIEvent], nextSequence: UInt64, hasMore: Bool
    )
    func uiEventCursor() async throws -> UInt64
    func acknowledgeUIEvents(through sequence: UInt64) async throws -> UInt64
}

extension NativeLocalAgentIPCClient: NativeLocalAgentEventClient {}

public enum NativeLocalAgentEventHubError: Error, Equatable, Sendable {
    case invalidPage(String)
    case invalidMainChatBinding(String)
    case cursorAcknowledgementMismatch(expected: UInt64, actual: UInt64)
}

extension NativeLocalAgentEventHubError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case let .invalidPage(reason):
            "本地 Agent 事件页无效：\(reason)"
        case let .invalidMainChatBinding(reason):
            "本地 Agent 主聊天绑定无效：\(reason)"
        case let .cursorAcknowledgementMismatch(expected, actual):
            "本地 Agent 事件游标确认不一致（期望 \(expected)，实际 \(actual)）"
        }
    }
}

public struct NativeLocalAgentEventDrainResult: Equatable, Sendable {
    public var initialSequence: UInt64
    public var acknowledgedSequence: UInt64
    public var appliedEventCount: Int

    public init(
        initialSequence: UInt64,
        acknowledgedSequence: UInt64,
        appliedEventCount: Int
    ) {
        self.initialSequence = initialSequence
        self.acknowledgedSequence = acknowledgedSequence
        self.appliedEventCount = appliedEventCount
    }
}

/// The only native event pump for one authenticated account.
///
/// It is intentionally independent from chat views: closing or switching a
/// page cannot stop Local Agent execution or cursor advancement.
public actor NativeLocalAgentEventHub {
    public typealias ClientProvider = @Sendable () async throws
        -> any NativeLocalAgentEventClient

    private enum Route: Sendable {
        case mainChat(LocalAgentMainChatRunBinding)
        case otherProfile
    }

    private static let pageLimit: UInt32 = 500
    private static let idleDelay: Duration = .milliseconds(350)
    private static let maximumFailureDelay: Duration = .seconds(15)

    private let clientProvider: ClientProvider
    private let sink: any LocalAgentUIEventApplying
    private var routes: [String: Route] = [:]
    private var worker: Task<Void, Never>?

    public init(
        clientProvider: @escaping ClientProvider,
        sink: any LocalAgentUIEventApplying
    ) {
        self.clientProvider = clientProvider
        self.sink = sink
    }

    deinit {
        worker?.cancel()
    }

    public func start() {
        guard worker == nil else { return }
        worker = Task { [weak self] in
            await self?.runLoop()
        }
    }

    public func stop() async {
        let running = worker
        worker = nil
        running?.cancel()
        _ = await running?.result
        routes.removeAll(keepingCapacity: false)
    }

    public func drainAvailableEvents() async throws -> NativeLocalAgentEventDrainResult {
        // Resolve the IPC endpoint for every drain. A supervised Host restart
        // creates a new endpoint; retaining the startup client would leave the
        // event loop permanently attached to the dead process.
        let client = try await clientProvider()
        let initial = try await client.uiEventCursor()
        var cursor = initial
        var count = 0

        while true {
            try Task.checkCancellation()
            let page = try await client.events(after: cursor, limit: Self.pageLimit)
            try validate(page: page, after: cursor)
            guard !page.events.isEmpty else {
                return NativeLocalAgentEventDrainResult(
                    initialSequence: initial,
                    acknowledgedSequence: cursor,
                    appliedEventCount: count
                )
            }

            for event in page.events {
                try Task.checkCancellation()
                let binding = try await mainChatBinding(for: event.event, client: client)
                try await sink.applyLocalAgentUIEvent(
                    event,
                    mainChatBinding: binding
                )
            }

            let through = try requiredLastSequence(in: page.events)
            let acknowledged = try await client.acknowledgeUIEvents(through: through)
            guard acknowledged == through else {
                throw NativeLocalAgentEventHubError.cursorAcknowledgementMismatch(
                    expected: through,
                    actual: acknowledged
                )
            }
            cursor = acknowledged
            count += page.events.count
            if !page.hasMore {
                return NativeLocalAgentEventDrainResult(
                    initialSequence: initial,
                    acknowledgedSequence: cursor,
                    appliedEventCount: count
                )
            }
        }
    }

    private func runLoop() async {
        var failures = 0
        while !Task.isCancelled {
            do {
                let result = try await drainAvailableEvents()
                failures = 0
                if result.appliedEventCount == 0 {
                    try await Task.sleep(for: Self.idleDelay)
                }
            } catch is CancellationError {
                return
            } catch {
                failures = min(failures + 1, 6)
                let seconds = min(1 << (failures - 1), 15)
                do {
                    try await Task.sleep(
                        for: min(.seconds(seconds), Self.maximumFailureDelay)
                    )
                } catch {
                    return
                }
            }
        }
    }

    private func mainChatBinding(
        for payload: LocalAgentUIEventPayload,
        client: any NativeLocalAgentEventClient
    ) async throws -> LocalAgentMainChatRunBinding? {
        guard let runID = payload.runID else { return nil }
        if let route = routes[runID] {
            switch route {
            case let .mainChat(binding): return binding
            case .otherProfile: return nil
            }
        }

        let run: LocalAgentRunSnapshot
        if case let .runSnapshot(snapshot) = payload {
            run = snapshot
        } else {
            run = try await client.run(id: runID)
        }
        guard run.runID == runID else {
            throw NativeLocalAgentEventHubError.invalidPage(
                "Run 查询返回了另一个 run_id"
            )
        }
        guard run.profileKey == "main_chat" else {
            routes[runID] = .otherProfile
            return nil
        }
        guard run.ownerEntityType == "conversation",
              run.ownerEntityID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
        else {
            throw NativeLocalAgentEventHubError.invalidMainChatBinding(
                "Run 不是有效的 conversation 所有者"
            )
        }

        let binding = try await client.mainChatRunBinding(runID: runID)
        guard binding.runID == runID,
              binding.threadID == run.ownerEntityID,
              !binding.turnID.isEmpty,
              !binding.messageID.isEmpty,
              binding.userMessage.recordID == binding.messageID,
              binding.userMessage.runID == binding.runID,
              binding.userMessage.threadID == binding.threadID,
              binding.userMessage.turnID == binding.turnID,
              binding.userMessage.role == .user,
              binding.userMessage.messageMode == .semantic,
              binding.userMessage.messageSource == "main_chat"
        else {
            throw NativeLocalAgentEventHubError.invalidMainChatBinding(
                "持久化消息身份与 Run 不一致"
            )
        }
        routes[runID] = .mainChat(binding)
        return binding
    }

    private func validate(
        page: (events: [LocalAgentUIEvent], nextSequence: UInt64, hasMore: Bool),
        after cursor: UInt64
    ) throws {
        guard !page.events.isEmpty else {
            guard page.nextSequence == cursor, !page.hasMore else {
                throw NativeLocalAgentEventHubError.invalidPage(
                    "空页推进了游标或声明仍有后续事件"
                )
            }
            return
        }

        var previous = cursor
        for event in page.events {
            guard event.eventSeq > previous else {
                throw NativeLocalAgentEventHubError.invalidPage(
                    "event_seq 未严格递增"
                )
            }
            previous = event.eventSeq
        }
        guard page.nextSequence == previous else {
            throw NativeLocalAgentEventHubError.invalidPage(
                "next_seq 不是页面最后一个 event_seq"
            )
        }
    }

    private func requiredLastSequence(in events: [LocalAgentUIEvent]) throws -> UInt64 {
        guard let sequence = events.last?.eventSeq else {
            throw NativeLocalAgentEventHubError.invalidPage("事件页为空")
        }
        return sequence
    }
}

private extension LocalAgentUIEventPayload {
    var runID: String? {
        switch self {
        case let .runSnapshot(run): run.runID
        case let .modelStream(event): event.runID
        case let .toolSnapshot(tool): tool.runID
        case let .userInteraction(interaction): interaction.runID
        case let .memorySync(status): status.runID
        case .hostStatus: nil
        }
    }
}
