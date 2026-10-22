import AppKit
import ChatOSCore
import CryptoKit
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class AgentMessageAttachmentImageLoaderTests: XCTestCase {
    func testVerifiedImageDecodesWithoutRemovingSource() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }

        let decoded = try await AgentMessageAttachmentImageLoader.load(
            payload(for: fixture, sha256: fixture.sha256),
            maximumDisplayPixelSize: 64
        )

        XCTAssertEqual(decoded.image.width, 64)
        XCTAssertEqual(decoded.image.height, 32)
        XCTAssertTrue(FileManager.default.fileExists(atPath: fixture.url.path))
    }

    func testRecordedSizeMismatchIsRejected() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }

        await assertInvalidAttachment(
            payload(for: fixture, size: fixture.data.count + 1, sha256: fixture.sha256)
        )
        XCTAssertTrue(FileManager.default.fileExists(atPath: fixture.url.path))
    }

    func testDigestMismatchIsRejected() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }

        await assertInvalidAttachment(
            payload(for: fixture, sha256: String(repeating: "0", count: 64))
        )
        XCTAssertTrue(FileManager.default.fileExists(atPath: fixture.url.path))
    }

    func testMissingDigestIsRejected() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }

        await assertInvalidAttachment(payload(for: fixture, sha256: nil))
        XCTAssertTrue(FileManager.default.fileExists(atPath: fixture.url.path))
    }

    func testThumbnailRetentionUsesDecodedMemoryCost() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let firstPayload = payload(for: fixture, sha256: fixture.sha256)
        let secondAttachment = ProjectAgentMessageAttachment(
            id: "attachment-2",
            name: "attachment-2.png",
            mimeType: "image/png",
            size: fixture.data.count,
            kind: .image,
            origin: .file,
            sha256: fixture.sha256
        )
        let decoded = try await AgentMessageAttachmentImageLoader.load(
            firstPayload,
            maximumDisplayPixelSize: 64
        )
        let first = AgentMessageAttachmentThumbnail(
            attachment: firstPayload.attachment,
            decoded: decoded
        )
        let second = AgentMessageAttachmentThumbnail(
            attachment: secondAttachment,
            decoded: decoded
        )
        let plan = [
            AgentAttachmentThumbnailLoadRequest(
                messageID: "message-1",
                attachment: firstPayload.attachment
            ),
            AgentAttachmentThumbnailLoadRequest(
                messageID: "message-2",
                attachment: secondAttachment
            ),
        ]

        let retained = AgentAttachmentThumbnailCachePolicy.retainedThumbnails(
            [firstPayload.attachment.id: first, secondAttachment.id: second],
            for: plan,
            maximumMemoryCost: first.memoryCost
        )

        XCTAssertEqual(Set(retained.keys), [firstPayload.attachment.id])
        XCTAssertEqual(
            AgentAttachmentThumbnailCachePolicy.remainingMemoryCost(
                after: retained,
                maximumMemoryCost: first.memoryCost
            ),
            0
        )
    }

    private func assertInvalidAttachment(
        _ payload: ProjectAgentMessageAttachmentPayload
    ) async {
        do {
            _ = try await AgentMessageAttachmentImageLoader.load(payload)
            XCTFail("Invalid image attachment was accepted")
        } catch AgentAttachmentPresentationError.invalidSize {
            // Expected.
        } catch {
            XCTFail("Unexpected error: \(error)")
        }
    }

    private func payload(
        for fixture: (root: URL, url: URL, data: Data, sha256: String),
        size: Int? = nil,
        sha256: String?
    ) -> ProjectAgentMessageAttachmentPayload {
        ProjectAgentMessageAttachmentPayload(
            attachment: .init(
                id: "attachment-1",
                name: "attachment.png",
                mimeType: "image/png",
                size: size ?? fixture.data.count,
                kind: .image,
                origin: .file,
                sha256: sha256
            ),
            localFileURL: fixture.url
        )
    }

    private func makeFixture() throws -> (root: URL, url: URL, data: Data, sha256: String) {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "AgentMessageAttachmentImageTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: 128, pixelsHigh: 64,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        bitmap.bitmapData?.initialize(repeating: 102, count: bitmap.bytesPerRow * bitmap.pixelsHigh)
        let data = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let url = root.appendingPathComponent("attachment.png")
        try data.write(to: url)
        let sha256 = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        return (root, url, data, sha256)
    }
}
