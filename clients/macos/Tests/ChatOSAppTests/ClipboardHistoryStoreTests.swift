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
        let store = ClipboardHistoryStore(rootURL: root)

        let first = try await store.add(
            payload: .text("hello clipboard"),
            contentHash: "hash-1",
            preview: "hello clipboard",
            sourceBundleID: "com.example.first"
        )
        let duplicate = try await store.add(
            payload: .text("hello clipboard"),
            contentHash: "hash-1",
            preview: "hello clipboard",
            sourceBundleID: "com.example.second"
        )

        #expect(first.id == duplicate.id)
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
        let store = ClipboardHistoryStore(rootURL: root)
        let files = [URL(fileURLWithPath: "/tmp/one.txt"), URL(fileURLWithPath: "/tmp/two.txt")]
        let fileEntry = try await store.add(
            payload: .files(files),
            contentHash: "files-hash",
            preview: "one.txt, two.txt",
            sourceBundleID: nil
        )
        let imageData = Data([0x89, 0x50, 0x4E, 0x47])
        let imageEntry = try await store.add(
            payload: .image(data: imageData, pasteboardType: "public.png"),
            contentHash: "image-hash",
            preview: nil,
            sourceBundleID: nil
        )

        #expect(try await store.payload(for: fileEntry) == .files(files))
        #expect(try await store.payload(for: imageEntry) == .image(
            data: imageData,
            pasteboardType: "public.png"
        ))
    }
}
