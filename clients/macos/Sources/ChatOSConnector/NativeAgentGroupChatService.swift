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
    private var openedStore: SQLiteAgentGroupChatStore?
    private var changeObservers: [UUID: ChangeObserver] = [:]

    public init(databaseURL: URL) {
        self.databaseURL = databaseURL
    }

    public func store() throws -> SQLiteAgentGroupChatStore {
        if let openedStore { return openedStore }
        let store = try SQLiteAgentGroupChatStore(databaseURL: databaseURL)
        openedStore = store
        return store
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
