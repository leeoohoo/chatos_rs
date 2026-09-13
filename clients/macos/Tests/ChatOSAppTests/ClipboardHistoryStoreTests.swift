import ChatOSCore
import Foundation
import Testing
@testable import ChatOSApp

struct ClipboardHistoryStoreTests {
    @Test
    func textPayloadPersistsDeduplicatesPinsAndDeletes() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("ChatOSClipboardTests-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let client = ClipboardHistoryIPCFake()
        let store = ClipboardHistoryStore(rootURL: root) {
            ClipboardHistoryStorageContext(ownerUserID: "user-1", client: client)
        }

        let first = try await store.add(
            payload: .text("hello clipboard"),
            contentHash: Self.digest("1"),
            preview: "hello clipboard",
            sourceBundleID: "com.example.first"
        )
        let duplicate = try await store.add(
            payload: .text("hello clipboard"),
            contentHash: Self.digest("1"),
            preview: "hello clipboard",
            sourceBundleID: "com.example.second"
        )

        #expect(first.id == duplicate.id)
        #expect(first.payloadReference.contains(
            "c6c289e49e9c05b2145860387b73bcb18df43fb09a1e4a4a9713c76c88bb541b"
        ))
        #expect(try await store.entries().count == 1)
        #expect(try await store.payload(for: duplicate) == .text("hello clipboard"))

        try await store.setPinned(true, id: first.id)
        #expect(try await store.entries().first?.isPinned == true)

        try await store.delete(id: first.id)
        #expect(try await store.entries().isEmpty)
    }

    @Test
    func fileAndImagePayloadsRoundTrip() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("ChatOSClipboardTests-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let client = ClipboardHistoryIPCFake()
        let store = ClipboardHistoryStore(rootURL: root) {
            ClipboardHistoryStorageContext(ownerUserID: "user-1", client: client)
        }
        let files = [URL(fileURLWithPath: "/tmp/one.txt"), URL(fileURLWithPath: "/tmp/two.txt")]
        let fileEntry = try await store.add(
            payload: .files(files),
            contentHash: Self.digest("2"),
            preview: "one.txt, two.txt",
            sourceBundleID: nil
        )
        let imageData = Data([0x89, 0x50, 0x4E, 0x47])
        let imageEntry = try await store.add(
            payload: .image(data: imageData, pasteboardType: "public.png"),
            contentHash: Self.digest("3"),
            preview: nil,
            sourceBundleID: nil
        )

        #expect(try await store.payload(for: fileEntry) == .files(files))
        #expect(try await store.payload(for: imageEntry) == .image(
            data: imageData,
            pasteboardType: "public.png"
        ))

        try await store.clear()
        #expect(try await store.entries().isEmpty)
        await #expect(throws: ClipboardHistoryStoreError.self) {
            _ = try await store.payload(for: fileEntry)
        }
    }

    @Test
    func payloadCannotBeReadThroughAnotherAccountContext() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("ChatOSClipboardTests-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let firstClient = ClipboardHistoryIPCFake(ownerUserID: "user-1")
        let firstStore = ClipboardHistoryStore(rootURL: root) {
            ClipboardHistoryStorageContext(ownerUserID: "user-1", client: firstClient)
        }
        let entry = try await firstStore.add(
            payload: .text("private clipboard"),
            contentHash: Self.digest("4"),
            preview: "private clipboard",
            sourceBundleID: nil
        )

        let secondClient = ClipboardHistoryIPCFake(ownerUserID: "user-2")
        let secondStore = ClipboardHistoryStore(rootURL: root) {
            ClipboardHistoryStorageContext(ownerUserID: "user-2", client: secondClient)
        }
        await #expect(throws: ClipboardHistoryStoreError.self) {
            _ = try await secondStore.payload(for: entry)
        }
    }

    private static func digest(_ character: Character) -> String {
        "sha256:" + String(repeating: String(character), count: 64)
    }
}

private actor ClipboardHistoryIPCFake: ClipboardHistoryIPCClient {
    private let ownerUserID: String
    private var entries: [String: LocalAgentClipboardSnapshot] = [:]
    private var clock: UInt64 = 0

    init(ownerUserID: String = "user-1") {
        self.ownerUserID = ownerUserID
    }

    func clipboardEntry(id: String) async throws -> LocalAgentClipboardSnapshot {
        guard let entry = entries[id] else { throw ClipboardHistoryStoreError.invalidRecord }
        return entry
    }

    func clipboardEntries() async throws -> [LocalAgentClipboardSnapshot] {
        entries.values.sorted {
            if $0.isPinned != $1.isPinned { return $0.isPinned }
            if $0.updatedAt != $1.updatedAt { return $0.updatedAt > $1.updatedAt }
            return $0.entryID < $1.entryID
        }
    }

    func storeClipboardEntry(
        id: String,
        draft: LocalAgentClipboardDraft
    ) async throws -> LocalAgentClipboardMutationResult {
        clock += 1
        let timestamp = Self.timestamp(clock)
        if var existing = entries.values.first(where: { $0.draft.contentHash == draft.contentHash }) {
            let discarded = existing.draft.payloadReference == draft.payloadReference
                ? []
                : [draft.payloadReference]
            existing.revision += 1
            existing.updatedAt = timestamp
            if draft.sourceBundleID != nil {
                existing.draft.sourceBundleID = draft.sourceBundleID
            }
            entries[existing.entryID] = existing
            return .init(entry: existing, discardedPayloadReferences: discarded)
        }
        let snapshot = LocalAgentClipboardSnapshot(
            entryID: id,
            ownerUserID: ownerUserID,
            draft: draft,
            revision: 1,
            isPinned: false,
            createdAt: timestamp,
            updatedAt: timestamp
        )
        entries[id] = snapshot
        return .init(entry: snapshot, discardedPayloadReferences: [])
    }

    func setClipboardEntryPinned(
        id: String,
        expectedRevision: UInt64,
        isPinned: Bool
    ) async throws -> LocalAgentClipboardMutationResult {
        guard var entry = entries[id], entry.revision == expectedRevision else {
            throw ClipboardHistoryStoreError.invalidRecord
        }
        clock += 1
        entry.revision += 1
        entry.isPinned = isPinned
        entry.updatedAt = Self.timestamp(clock)
        entries[id] = entry
        return .init(entry: entry, discardedPayloadReferences: [])
    }

    func deleteClipboardEntry(
        id: String,
        expectedRevision: UInt64
    ) async throws -> LocalAgentClipboardMutationResult {
        guard let entry = entries[id], entry.revision == expectedRevision else {
            throw ClipboardHistoryStoreError.invalidRecord
        }
        entries[id] = nil
        return .init(entry: nil, discardedPayloadReferences: [entry.draft.payloadReference])
    }

    private static func timestamp(_ value: UInt64) -> String {
        String(format: "2026-09-13T00:00:%02lluZ", value)
    }
}
