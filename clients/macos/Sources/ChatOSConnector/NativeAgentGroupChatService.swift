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
    private let agentArtifactService: (any AgentArtifactRemoteServing)?
    private var openedStore: SQLiteAgentGroupChatStore?
    private var changeObservers: [UUID: ChangeObserver] = [:]

    public init(
        databaseURL: URL,
        agentArtifactService: (any AgentArtifactRemoteServing)? = nil
    ) {
        self.databaseURL = databaseURL
        self.agentArtifactService = agentArtifactService
    }

    public func store() throws -> SQLiteAgentGroupChatStore {
        if let openedStore { return openedStore }
        let store = try SQLiteAgentGroupChatStore(
            databaseURL: databaseURL,
            agentArtifactService: agentArtifactService
        )
        openedStore = store
        return store
    }

    @discardableResult
    public func syncPendingAgentArtifacts(
        ownerUserID: String,
        limit: Int = 8
    ) async throws -> Int {
        guard let agentArtifactService else { return 0 }
        guard (1...32).contains(limit) else {
            throw AgentGroupChatError.invalidField("limit")
        }
        let store = try store()
        var completed = 0
        for _ in 0..<limit {
            try Task.checkCancellation()
            let now = Int64(Date().timeIntervalSince1970 * 1_000)
            guard let job = try await store.claimNextAgentArtifactUpload(
                ownerUserID: ownerUserID,
                nowUnixMs: now
            ) else { break }
            do {
                let metadata = try await agentArtifactService.upload(job.request)
                guard metadata.sha256 == job.request.sha256,
                      metadata.size == job.request.data.count else {
                    throw AgentGroupChatError.storage("Agent artifact metadata mismatch")
                }
                try await store.markAgentArtifactUploadSynced(
                    ownerUserID: ownerUserID,
                    attachmentID: job.attachmentID,
                    metadata: metadata,
                    nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
                )
                try? await store.recordAgentArtifactUpload(
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
                try await store.markAgentArtifactUploadFailed(
                    ownerUserID: ownerUserID,
                    attachmentID: job.attachmentID,
                    attempt: job.attempt,
                    error: "云端同步暂时失败，请稍后重试。",
                    nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
                )
                try? await store.recordAgentArtifactUpload(
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

    public func retryAgentArtifactUpload(
        ownerUserID: String,
        roomID: String,
        attachmentID: String
    ) async throws {
        let store = try store()
        try await store.retryAgentArtifactUpload(
            ownerUserID: ownerUserID,
            attachmentID: attachmentID
        )
        publishChange(.init(
            ownerUserID: ownerUserID,
            roomID: roomID,
            kind: .roomUpdated
        ))
        _ = try await syncPendingAgentArtifacts(ownerUserID: ownerUserID, limit: 1)
    }

    /// Emits process-local invalidations after the durable SQLite write has completed. Consumers
    /// always re-read SQLite, so this stream is only a wake-up signal and never a second source of
    /// truth. `bufferingNewest` coalesces rapid model/tool checkpoint updates for slow UI readers.
    public func changes(
        ownerUserID: String,
        roomID: String? = nil
    ) -> AsyncStream<NativeAgentGroupChatChange> {
        AsyncStream(bufferingPolicy: .bufferingNewest(1)) { continuation in
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
