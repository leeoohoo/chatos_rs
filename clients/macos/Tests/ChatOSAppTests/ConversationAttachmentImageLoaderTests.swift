import AppKit
import ChatOSMediaWorkerProtocol
import CryptoKit
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class ConversationAttachmentImageLoaderTests: XCTestCase {
    func testVerifiedImageDecodesFromFileWithoutRemovingSource() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }

        let decoded = try await ConversationAttachmentImageLoader.load(
            url: fixture.url,
            sha256: fixture.sha256,
            expectedBytes: fixture.data.count,
            ownerUserID: "test-user",
            maximumDisplayPixelSize: 64,
            temporaryDirectory: fixture.root,
            availableCapacityOverride: Int64.max,
            mediaWorker: TestMediaWorkerImageNormalizer()
        )

        XCTAssertEqual(decoded.image.width, 64)
        XCTAssertEqual(decoded.image.height, 32)
        XCTAssertTrue(FileManager.default.fileExists(atPath: fixture.url.path))
    }

    func testDigestMismatchIsRejected() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }

        do {
            _ = try await ConversationAttachmentImageLoader.load(
                url: fixture.url,
                sha256: String(repeating: "0", count: 64),
                expectedBytes: fixture.data.count,
                ownerUserID: "test-user",
                temporaryDirectory: fixture.root,
                availableCapacityOverride: Int64.max,
                mediaWorker: TestMediaWorkerImageNormalizer()
            )
            XCTFail("Mismatched digest was accepted")
        } catch {
            XCTAssertTrue(FileManager.default.fileExists(atPath: fixture.url.path))
        }
    }

    func testSymbolicLinkIsRejected() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let link = fixture.root.appendingPathComponent("linked.png")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: fixture.url)

        do {
            _ = try await ConversationAttachmentImageLoader.load(
                url: link,
                sha256: fixture.sha256,
                expectedBytes: fixture.data.count,
                ownerUserID: "test-user",
                temporaryDirectory: fixture.root,
                availableCapacityOverride: Int64.max,
                mediaWorker: TestMediaWorkerImageNormalizer()
            )
            XCTFail("Symbolic link was accepted")
        } catch {
            XCTAssertTrue(FileManager.default.fileExists(atPath: fixture.url.path))
        }
    }

    func testSourceReplacementDuringWorkerOperationIsRejected() async throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }

        do {
            _ = try await ConversationAttachmentImageLoader.load(
                url: fixture.url,
                sha256: fixture.sha256,
                expectedBytes: fixture.data.count,
                ownerUserID: "test-user",
                maximumDisplayPixelSize: 64,
                temporaryDirectory: fixture.root,
                availableCapacityOverride: Int64.max,
                mediaWorker: ReplacingTestMediaWorker()
            )
            XCTFail("A source replaced after digest verification was accepted")
        } catch {
            XCTAssertTrue(FileManager.default.fileExists(atPath: fixture.url.path))
        }
    }

    private func makeFixture() throws -> (root: URL, url: URL, data: Data, sha256: String) {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "ConversationAttachmentImageTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: 128, pixelsHigh: 64,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        bitmap.bitmapData?.initialize(repeating: 85, count: bitmap.bytesPerRow * bitmap.pixelsHigh)
        let data = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let url = root.appendingPathComponent("attachment.png")
        try data.write(to: url)
        let sha256 = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        return (root, url, data, sha256)
    }
}

private struct ReplacingTestMediaWorker: MediaWorkerImageNormalizing {
    func normalizePNG(
        sourceURL: URL,
        sourceIdentity: MediaWorkerFileIdentity?,
        destinationURL: URL,
        ownerUserID: String,
        maximumSourceBytes: Int,
        maximumSourcePixelCount: Int,
        maximumOutputPixelSize: Int,
        maximumOutputBytes: Int
    ) async throws -> MediaWorkerImageDescriptor {
        let result = try await TestMediaWorkerImageNormalizer().normalizePNG(
            sourceURL: sourceURL,
            sourceIdentity: sourceIdentity,
            destinationURL: destinationURL,
            ownerUserID: ownerUserID,
            maximumSourceBytes: maximumSourceBytes,
            maximumSourcePixelCount: maximumSourcePixelCount,
            maximumOutputPixelSize: maximumOutputPixelSize,
            maximumOutputBytes: maximumOutputBytes
        )
        let replacement = sourceURL.deletingLastPathComponent().appendingPathComponent(
            "replacement-\(UUID().uuidString).png"
        )
        try Data(contentsOf: sourceURL).write(to: replacement)
        _ = try FileManager.default.replaceItemAt(sourceURL, withItemAt: replacement)
        return result
    }
}
