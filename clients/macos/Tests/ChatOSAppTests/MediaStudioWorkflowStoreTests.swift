import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSApp

final class MediaStudioWorkflowStoreTests: XCTestCase {
    func testWorkflowAndImagesRoundTripAndStayAccountScoped() async throws {
        let root = try temporaryDirectory()
        let source = root.appendingPathComponent("source.png")
        try Self.png.write(to: source)
        let store = MediaStudioWorkflowStore(root: root.appendingPathComponent("store"))
        var workflow = Self.workflow()
        try await store.save(workflow, owner: "alice")

        let reference = try await store.importImage(
            .init(name: "source.png", mimeType: "image/png", fileURL: source),
            workflowID: workflow.id,
            owner: "alice"
        )
        workflow.inputImages = [reference]
        try await store.save(workflow, owner: "alice")

        let restored = try await MediaStudioWorkflowStore(
            root: root.appendingPathComponent("store")
        ).list(owner: "alice")
        XCTAssertEqual(restored, [workflow])
        let otherAccount = try await store.list(owner: "bob")
        XCTAssertTrue(otherAccount.isEmpty)
        let images = try await store.resolveImages(
            restored[0].inputImages,
            workflowID: workflow.id,
            owner: "alice"
        )
        XCTAssertEqual(try Data(contentsOf: XCTUnwrap(images[0].fileURL)), Self.png)
    }

    func testSavingRemovesOnlyUnreferencedAssets() async throws {
        let root = try temporaryDirectory()
        let store = MediaStudioWorkflowStore(root: root)
        var workflow = Self.workflow()
        try await store.save(workflow, owner: "alice")
        let source = root.appendingPathComponent("source.png")
        try Self.png.write(to: source)
        let reference = try await store.importImage(
            .init(name: "source.png", mimeType: "image/png", fileURL: source),
            workflowID: workflow.id,
            owner: "alice"
        )
        workflow.inputImages = [reference]
        try await store.save(workflow, owner: "alice")
        workflow.inputImages = []
        try await store.save(workflow, owner: "alice")

        do {
            _ = try await store.resolveImages([reference], workflowID: workflow.id, owner: "alice")
            XCTFail("An unreferenced asset should be removed")
        } catch {}
    }

    func testRejectsEscapingImageFilenameInManifest() async throws {
        let root = try temporaryDirectory()
        let store = MediaStudioWorkflowStore(root: root)
        var workflow = Self.workflow()
        workflow.inputImages = [.init(
            name: "escape.png",
            mimeType: "image/png",
            filename: "../escape.png",
            byteCount: 10
        )]

        do {
            try await store.save(workflow, owner: "alice")
            XCTFail("A path traversal reference should fail closed")
        } catch {}
    }

    func testOlderSaveCannotOverwriteNewerWorkflowSnapshot() async throws {
        let root = try temporaryDirectory()
        let store = MediaStudioWorkflowStore(root: root)
        let older = Self.workflow()
        var newer = older
        newer.nodes.append(.init(kind: .image, modelConfigID: "image", prompt: "Render it"))
        newer.updatedAt = older.updatedAt.addingTimeInterval(1)

        let savedNewer = try await store.save(newer, owner: "alice")
        let savedOlder = try await store.save(older, owner: "alice")
        XCTAssertTrue(savedNewer)
        XCTAssertFalse(savedOlder)

        let restored = try await store.list(owner: "alice")
        XCTAssertEqual(restored, [newer])
    }

    func testLoadingMigratesLegacyVideoOptions() async throws {
        let root = try temporaryDirectory()
        let store = MediaStudioWorkflowStore(root: root)
        var workflow = MediaStudioWorkflow(
            name: "Legacy Video",
            nodes: [.init(kind: .video, modelConfigID: "video", prompt: "Animate it")]
        )
        try await store.save(workflow, owner: "alice")
        let enumerator = try XCTUnwrap(FileManager.default.enumerator(at: root, includingPropertiesForKeys: nil))
        let manifest = try XCTUnwrap(
            enumerator.compactMap { $0 as? URL }.first { $0.lastPathComponent == "workflow.json" }
        )
        workflow.nodes[0].videoSize = "1280x720"
        workflow.nodes[0].videoSeconds = 120
        try JSONEncoder().encode(workflow).write(to: manifest, options: .atomic)

        let restored = try await store.list(owner: "alice")

        XCTAssertEqual(restored.first?.nodes.first?.videoSize, "768P")
        XCTAssertEqual(restored.first?.nodes.first?.videoSeconds, 15)
    }

    func testStaleSaveDoesNotDeleteAnImageBeingAddedToNewerSnapshot() async throws {
        let root = try temporaryDirectory()
        let store = MediaStudioWorkflowStore(root: root.appendingPathComponent("store"))
        let source = root.appendingPathComponent("source.png")
        try Self.png.write(to: source)
        let older = Self.workflow()
        try await store.save(older, owner: "alice")
        let reference = try await store.importImage(
            .init(name: "source.png", mimeType: "image/png", fileURL: source),
            workflowID: older.id,
            owner: "alice"
        )

        try await store.save(older, owner: "alice")
        var newer = older
        newer.inputImages = [reference]
        newer.updatedAt = older.updatedAt.addingTimeInterval(1)
        try await store.save(newer, owner: "alice")

        let restored = try await store.list(owner: "alice")
        XCTAssertEqual(restored, [newer])
    }

    private func temporaryDirectory() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("MediaStudioWorkflowTests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url
    }

    private static func workflow() -> MediaStudioWorkflow {
        .init(
            name: "Campaign",
            inputPrompt: "A launch campaign",
            nodes: [.init(kind: .text, modelConfigID: "chat", prompt: "Write a prompt")]
        )
    }

    private static let png = Data(base64Encoded:
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
    )!
}
