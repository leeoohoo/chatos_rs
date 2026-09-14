import ChatOSCore
import ChatOSConnector
import Foundation
@testable import ChatOSApp

actor StoryProjectTestBackend {
    private var records: [String: [String: LocalAgentStorySnapshot]] = [:]
    private var sequence: UInt64 = 0

    func client(owner: String) -> StoryProjectTestClient {
        StoryProjectTestClient(owner: owner, backend: self)
    }

    func record(owner: String, id: String) throws -> LocalAgentStorySnapshot {
        guard let record = records[owner]?[id] else {
            throw NativeLocalAgentIPCError.rejected(.init(
                code: "story_not_found",
                message: "Story record was not found",
                retryable: false
            ))
        }
        return record
    }

    func all(owner: String) -> [LocalAgentStorySnapshot] {
        Array(records[owner, default: [:]].values)
    }

    func put(
        owner: String,
        id: String,
        expectedRevision: UInt64?,
        draft: LocalAgentStoryDraft
    ) throws -> LocalAgentStorySnapshot {
        let current = records[owner]?[id]
        guard current?.revision == expectedRevision else {
            throw NativeLocalAgentIPCError.rejected(.init(
                code: "story_revision_conflict",
                message: "Story revision conflict",
                retryable: false
            ))
        }
        sequence += 1
        let timestamp = String(format: "2026-09-13T00:02:%02lluZ", sequence % 60)
        let snapshot = LocalAgentStorySnapshot(
            recordID: id,
            ownerUserID: owner,
            draft: draft,
            revision: (current?.revision ?? 0) + 1,
            createdAt: current?.createdAt ?? timestamp,
            updatedAt: timestamp
        )
        records[owner, default: [:]][id] = snapshot
        return snapshot
    }

    func delete(owner: String, id: String, expectedRevision: UInt64) throws {
        guard let current = records[owner]?[id], current.revision == expectedRevision else {
            throw NativeLocalAgentIPCError.rejected(.init(
                code: "story_revision_conflict",
                message: "Story revision conflict",
                retryable: false
            ))
        }
        records[owner]?[id] = nil
    }

    func replace(owner: String, record: LocalAgentStorySnapshot) {
        records[owner, default: [:]][record.recordID] = record
    }
}

struct StoryProjectTestClient: StoryProjectIPCClient {
    let owner: String
    let backend: StoryProjectTestBackend

    func run(id: String) async throws -> LocalAgentRunSnapshot {
        throw StoryAgentError.unavailable
    }

    func runs(cursor: String?, limit: UInt32) async throws -> (
        runs: [LocalAgentRunSnapshot], nextCursor: String?
    ) {
        ([], nil)
    }

    func accepted(_ command: LocalAgentCommand) async throws -> String {
        throw StoryAgentError.unavailable
    }

    func createStoryDesign(
        _ command: LocalAgentCreateStoryDesign
    ) async throws -> (operationID: String, run: LocalAgentRunSnapshot) {
        throw StoryAgentError.unavailable
    }

    func applyStoryDesign(
        _ command: LocalAgentApplyStoryDesign
    ) async throws -> LocalAgentStoryDesignApplication {
        throw StoryAgentError.unavailable
    }

    func storyRecord(id: String) async throws -> LocalAgentStorySnapshot {
        try await backend.record(owner: owner, id: id)
    }

    func storyRecords() async throws -> [LocalAgentStorySnapshot] {
        await backend.all(owner: owner)
    }

    func putStoryRecord(
        id: String,
        expectedRevision: UInt64?,
        draft: LocalAgentStoryDraft
    ) async throws -> LocalAgentStorySnapshot {
        try await backend.put(
            owner: owner,
            id: id,
            expectedRevision: expectedRevision,
            draft: draft
        )
    }

    func deleteStoryRecord(id: String, expectedRevision: UInt64) async throws {
        try await backend.delete(owner: owner, id: id, expectedRevision: expectedRevision)
    }
}

func makeStoryProjectStore(
    root: URL,
    backend: StoryProjectTestBackend = StoryProjectTestBackend()
) -> StoryProjectStore {
    StoryProjectStore(root: root) { owner in
        StoryProjectStorageContext(
            ownerUserID: owner,
            client: await backend.client(owner: owner)
        )
    }
}
