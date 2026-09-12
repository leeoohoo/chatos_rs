// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

public protocol NativeLocalAgentMainChatRestoreClient: Sendable {
    func runs(cursor: String?, limit: UInt32) async throws -> (
        runs: [LocalAgentRunSnapshot], nextCursor: String?
    )
    func runDetail(
        id: String,
        eventLimit: UInt32,
        eventOffset: UInt32
    ) async throws -> LocalAgentRunDetail
    func mainChatRunBinding(runID: String) async throws -> LocalAgentMainChatRunBinding
}

extension NativeLocalAgentIPCClient: NativeLocalAgentMainChatRestoreClient {}

public enum NativeLocalAgentMainChatRestoreError: Error, Equatable, Sendable {
    case paginationDidNotAdvance(String)
    case invalidRunDetail(String)
    case duplicateTimelineEvent(String)
}

extension NativeLocalAgentMainChatRestoreError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case let .paginationDidNotAdvance(kind):
            "本地 Agent \(kind) 分页没有推进"
        case let .invalidRunDetail(runID):
            "本地 Agent Run 恢复快照不一致：\(runID)"
        case let .duplicateTimelineEvent(eventID):
            "本地 Agent Run 恢复快照包含重复事件：\(eventID)"
        }
    }
}

/// Rebuilds Main Chat presentation state from durable Host records before the
/// account-level incremental cursor starts. It never replays model or tool
/// side effects and does not read remote compact conversation history.
public actor NativeLocalAgentMainChatRestorer {
    public typealias ClientProvider = @Sendable () async throws
        -> any NativeLocalAgentMainChatRestoreClient

    private static let pageLimit: UInt32 = 500

    private let clientProvider: ClientProvider
    private let store: any LocalAgentMainChatStateRestoring

    public init(
        clientProvider: @escaping ClientProvider,
        store: any LocalAgentMainChatStateRestoring
    ) {
        self.clientProvider = clientProvider
        self.store = store
    }

    public func restore() async throws {
        // One restore attempt stays on one Host endpoint so paginated Run
        // details cannot splice snapshots from different Host lifetimes.
        let client = try await clientProvider()
        try await restore(using: client)
    }

    func restore(using client: any NativeLocalAgentMainChatRestoreClient) async throws {
        let allRuns = try await allRuns(client: client)
        let mainChatRuns = try allRuns.filter { run in
            guard run.profileKey == "main_chat" else { return false }
            guard run.ownerEntityType == "conversation" else {
                throw NativeLocalAgentMainChatRestoreError.invalidRunDetail(run.runID)
            }
            return true
        }
        var recoveries: [LocalAgentMainChatRunRecovery] = []
        recoveries.reserveCapacity(mainChatRuns.count)

        for listedRun in mainChatRuns {
            async let binding = client.mainChatRunBinding(runID: listedRun.runID)
            async let detail = completeDetail(runID: listedRun.runID, client: client)
            let recovery = try await LocalAgentMainChatRunRecovery(
                binding: binding,
                detail: detail
            )
            guard recovery.binding.runID == listedRun.runID,
                  recovery.detail.run.runID == listedRun.runID
            else {
                throw NativeLocalAgentMainChatRestoreError.invalidRunDetail(listedRun.runID)
            }
            recoveries.append(recovery)
        }
        try await store.restoreLocalAgentMainChatRuns(recoveries)
    }

    private func allRuns(
        client: any NativeLocalAgentMainChatRestoreClient
    ) async throws -> [LocalAgentRunSnapshot] {
        var cursor: String?
        var values: [LocalAgentRunSnapshot] = []
        var runIDs = Set<String>()
        repeat {
            let page = try await client.runs(cursor: cursor, limit: Self.pageLimit)
            for run in page.runs {
                guard runIDs.insert(run.runID).inserted else {
                    throw NativeLocalAgentMainChatRestoreError.invalidRunDetail(run.runID)
                }
                values.append(run)
            }
            if let next = page.nextCursor, next == cursor {
                throw NativeLocalAgentMainChatRestoreError.paginationDidNotAdvance("Run")
            }
            cursor = page.nextCursor
        } while cursor != nil
        return values
    }

    private func completeDetail(
        runID: String,
        client: any NativeLocalAgentMainChatRestoreClient
    ) async throws -> LocalAgentRunDetail {
        var offset: UInt32 = 0
        var latest: LocalAgentRunDetail?
        var events: [LocalAgentRunTimelineEvent] = []
        var eventIDs = Set<String>()
        var snapshotEventSequence: UInt64 = 0

        while true {
            let page = try await client.runDetail(
                id: runID,
                eventLimit: Self.pageLimit,
                eventOffset: offset
            )
            guard page.run.runID == runID,
                  UInt32(events.count) == offset,
                  page.snapshotEventSequence >= snapshotEventSequence,
                  page.eventsTotal >= offset + UInt32(page.events.count)
            else {
                throw NativeLocalAgentMainChatRestoreError.invalidRunDetail(runID)
            }
            // A Run may continue while its durable timeline is being paged.
            // Each Host response is internally consistent, but later pages are
            // allowed to expose a newer Run/tool snapshot and a larger total.
            latest = page
            snapshotEventSequence = page.snapshotEventSequence
            for event in page.events {
                guard eventIDs.insert(event.eventID).inserted else {
                    throw NativeLocalAgentMainChatRestoreError.duplicateTimelineEvent(event.eventID)
                }
                events.append(event)
            }
            let nextOffset = offset + UInt32(page.events.count)
            guard page.eventsHasMore else {
                guard nextOffset == page.eventsTotal, var result = latest else {
                    throw NativeLocalAgentMainChatRestoreError.invalidRunDetail(runID)
                }
                result.events = events
                result.eventsHasMore = false
                return result
            }
            guard nextOffset > offset else {
                throw NativeLocalAgentMainChatRestoreError.paginationDidNotAdvance("Run 事件")
            }
            offset = nextOffset
        }
    }
}
