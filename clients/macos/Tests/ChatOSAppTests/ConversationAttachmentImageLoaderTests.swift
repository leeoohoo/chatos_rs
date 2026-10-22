import AppKit
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
            maximumDisplayPixelSize: 64
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
                expectedBytes: fixture.data.count
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
                expectedBytes: fixture.data.count
            )
            XCTFail("Symbolic link was accepted")
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
