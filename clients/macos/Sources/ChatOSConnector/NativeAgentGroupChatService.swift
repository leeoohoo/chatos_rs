import ChatOSCore
import Foundation

public struct NativeAgentGroupChatChange: Sendable, Equatable {
    public enum Kind: String, Sendable {
        case deliveryClaimed = "delivery_claimed"
        case runUpdated = "run_updated"
        case roomUpdated = "room_updated"
    }

    public let ownerUserID: String
    public let roomID: String
    public let agentID: String?
    public let runID: UUID?
    public let kind: Kind

    public init(
        ownerUserID: String,
        roomID: String,
        agentID: String? = nil,
        runID: UUID? = nil,
        kind: Kind
    ) {
        self.ownerUserID = ownerUserID
        self.roomID = roomID
        self.agentID = agentID
        self.runID = runID
        self.kind = kind
    }
}

/// Lazily opens the account-scoped local Agent chat database. No remote service is required.
public actor NativeAgentGroupChatService {
    private struct ChangeObserver {
        let ownerUserID: String
        let roomID: String?
        let continuation: AsyncStream<NativeAgentGroupChatChange>.Continuation
    }

    private let databaseURL: URL
    private let agentArtifactStore: (any AgentArtifactServing)?
    private var openedStore: SQLiteAgentGroupChatStore?
    private var changeObservers: [UUID: ChangeObserver] = [:]

    public init(
        databaseURL: URL,
        agentArtifactStore: (any AgentArtifactServing)? = nil
    ) {
        self.databaseURL = databaseURL
        self.agentArtifactStore = agentArtifactStore
    }

    public func store() throws -> SQLiteAgentGroupChatStore {
        if let openedStore { return openedStore }
        let store = try SQLiteAgentGroupChatStore(
            databaseURL: databaseURL,
            agentArtifactStore: agentArtifactStore
        )
        openedStore = store
        return store
    }

    @discardableResult
    public func persistPendingAgentArtifacts(
        ownerUserID: String,
        limit: Int = 8
    ) async throws -> Int {
        guard let agentArtifactStore else { return 0 }
        guard (1...32).contains(limit) else {
            throw AgentGroupChatError.invalidField("limit")
        }
        let store = try store()
        var completed = 0
        for _ in 0..<limit {
            try Task.checkCancellation()
            let now = Int64(Date().timeIntervalSince1970 * 1_000)
            guard let job = try await store.claimNextAgentArtifactStorage(
                ownerUserID: ownerUserID,
                nowUnixMs: now
            ) else { break }
            do {
                let metadata = try await agentArtifactStore.store(
                    ownerUserID: ownerUserID,
                    request: job.request
                )
                guard metadata.sha256 == job.request.sha256,
                      metadata.size == job.request.data.count else {
                    throw AgentGroupChatError.storage("Agent artifact metadata mismatch")
                }
                try await store.markAgentArtifactStored(
                    ownerUserID: ownerUserID,
                    attachmentID: job.attachmentID,
                    metadata: metadata,
                    nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
                )
                try? await store.recordAgentArtifactStorage(
                    ownerUserID: ownerUserID,
                    outcome: .succeeded,
                    bytes: job.request.data.count,
                    nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
                )
                publishChange(.init(
                    ownerUserID: ownerUserID,
                    roomID: job.roomID,
                    kind: .roomUpdated
                ))
                completed += 1
            } catch is CancellationError {
                throw CancellationError()
            } catch {
                try await store.markAgentArtifactStorageFailed(
                    ownerUserID: ownerUserID,
                    attachmentID: job.attachmentID,
                    attempt: job.attempt,
                    error: "本地文档存储暂时失败，请稍后重试。",
                    nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
                )
                try? await store.recordAgentArtifactStorage(
                    ownerUserID: ownerUserID,
                    outcome: .failed,
                    bytes: job.request.data.count,
                    nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
                )
                publishChange(.init(
                    ownerUserID: ownerUserID,
                    roomID: job.roomID,
                    kind: .roomUpdated
                ))
            }
        }
        return completed
    }

    public func retryAgentArtifactStorage(
        ownerUserID: String,
        roomID: String,
        attachmentID: String
    ) async throws {
        let store = try store()
        try await store.retryAgentArtifactStorage(
            ownerUserID: ownerUserID,
            attachmentID: attachmentID
        )
        publishChange(.init(
            ownerUserID: ownerUserID,
            roomID: roomID,
            kind: .roomUpdated
        ))
        _ = try await persistPendingAgentArtifacts(ownerUserID: ownerUserID, limit: 1)
    }

    public func localAgentArtifacts(
        ownerUserID: String,
        limit: Int = 50,
        cursor: String? = nil
    ) async throws -> AgentArtifactPage {
        guard let agentArtifactStore else {
            throw AgentGroupChatError.storage("Local Agent artifact store is unavailable")
        }
        return try await agentArtifactStore.list(
            ownerUserID: ownerUserID,
            limit: limit,
            cursor: cursor
        )
    }

    public func localAgentArtifactData(
        ownerUserID: String,
        artifactID: String
    ) async throws -> Data {
        guard let agentArtifactStore else {
            throw AgentGroupChatError.storage("Local Agent artifact store is unavailable")
        }
        return try await agentArtifactStore.read(
            ownerUserID: ownerUserID,
            artifactID: artifactID
        )
    }

    /// Emits process-local invalidations after the durable SQLite write has completed. Consumers
    /// always re-read SQLite, so this stream is only a wake-up signal and never a second source of
    /// truth. A small bounded buffer preserves a lower-frequency `roomUpdated` invalidation among
    /// rapid model/tool checkpoints while still applying backpressure to slow UI readers.
    public func changes(
        ownerUserID: String,
        roomID: String? = nil
    ) -> AsyncStream<NativeAgentGroupChatChange> {
        AsyncStream(bufferingPolicy: .bufferingNewest(64)) { continuation in
            let id = UUID()
            changeObservers[id] = .init(
                ownerUserID: ownerUserID,
                roomID: roomID,
                continuation: continuation
            )
            continuation.onTermination = { [weak self] _ in
                Task { await self?.removeChangeObserver(id) }
            }
        }
    }

    public func publishChange(_ change: NativeAgentGroupChatChange) {
        for observer in changeObservers.values
        where observer.ownerUserID == change.ownerUserID
            && (observer.roomID == nil || observer.roomID == change.roomID) {
            observer.continuation.yield(change)
        }
    }

    private func removeChangeObserver(_ id: UUID) {
        changeObservers.removeValue(forKey: id)
    }
}
