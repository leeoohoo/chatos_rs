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
