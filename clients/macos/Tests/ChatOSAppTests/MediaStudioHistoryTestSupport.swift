import ChatOSAPI
import ChatOSCore
import Foundation
@testable import ChatOSApp

actor MediaStudioHistoryTestBackend {
    private var records: [String: [String: LocalAgentMediaSnapshot]] = [:]
    private var sequence: UInt64 = 0

    func client(owner: String) -> MediaStudioHistoryTestClient {
        MediaStudioHistoryTestClient(owner: owner, backend: self)
    }

    func record(owner: String, id: String) throws -> LocalAgentMediaSnapshot {
        guard let record = records[owner]?[id] else { throw HistoryError.invalidRecord }
        return record
    }

    func all(owner: String) -> [LocalAgentMediaSnapshot] {
        Array(records[owner, default: [:]].values)
    }

    func put(
        owner: String,
        id: String,
        expectedRevision: UInt64?,
        draft: LocalAgentMediaDraft
    ) throws -> LocalAgentMediaMutationResult {
        let current = records[owner]?[id]
        guard current?.revision == expectedRevision else { throw HistoryError.invalidRecord }
        sequence += 1
        let timestamp = String(format: "2026-09-13T00:01:%02lluZ", sequence % 60)
        let previous = Set(current?.draft.assets.map(\.payloadReference) ?? [])
        let next = Set(draft.assets.map(\.payloadReference))
        let snapshot = LocalAgentMediaSnapshot(
            recordID: id,
            ownerUserID: owner,
            draft: draft,
            revision: (current?.revision ?? 0) + 1,
            createdAt: current?.createdAt ?? timestamp,
            updatedAt: timestamp
        )
        records[owner, default: [:]][id] = snapshot
        return .init(
            record: snapshot,
            discardedPayloadReferences: Array(previous.subtracting(next)).sorted()
        )
    }

    func delete(
        owner: String,
        id: String,
        expectedRevision: UInt64
    ) throws -> LocalAgentMediaMutationResult {
        guard let current = records[owner]?[id], current.revision == expectedRevision else {
            throw HistoryError.invalidRecord
        }
        records[owner]?[id] = nil
        return .init(
            record: nil,
            discardedPayloadReferences: current.draft.assets.map(\.payloadReference).sorted()
        )
    }
}

struct MediaStudioHistoryTestClient: MediaStudioHistoryIPCClient {
    let owner: String
    let backend: MediaStudioHistoryTestBackend

    func mediaRecord(id: String) async throws -> LocalAgentMediaSnapshot {
        try await backend.record(owner: owner, id: id)
    }

    func mediaRecords() async throws -> [LocalAgentMediaSnapshot] {
        await backend.all(owner: owner)
    }

    func putMediaRecord(
        id: String,
        expectedRevision: UInt64?,
        draft: LocalAgentMediaDraft
    ) async throws -> LocalAgentMediaMutationResult {
        try await backend.put(
            owner: owner,
            id: id,
            expectedRevision: expectedRevision,
            draft: draft
        )
    }

    func deleteMediaRecord(
        id: String,
        expectedRevision: UInt64
    ) async throws -> LocalAgentMediaMutationResult {
        try await backend.delete(owner: owner, id: id, expectedRevision: expectedRevision)
    }
}

func makeMediaStudioHistoryStore(
    root: URL,
    transport: any HTTPTransport = URLSessionHTTPTransport(),
    backend: MediaStudioHistoryTestBackend = MediaStudioHistoryTestBackend()
) -> MediaStudioHistoryStore {
    MediaStudioHistoryStore(root: root, transport: transport) { owner in
        MediaStudioHistoryStorageContext(
            ownerUserID: owner,
            client: await backend.client(owner: owner)
        )
    }
}
