import AppKit
import Foundation
import XCTest
@testable import ChatOSApp

final class MediaStudioVideoInputArtifactStoreTests: XCTestCase {
    private func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(
            "MediaStudioVideoInputArtifactStoreTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url
    }

    func testImageSelectionUsesPrivateFileInsteadOfBase64() async throws {
        let root = try directory()
        let source = root.appendingPathComponent("selected.png")
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil,
            pixelsWide: 256,
            pixelsHigh: 256,
            bitsPerSample: 8,
            samplesPerPixel: 4,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: .deviceRGB,
            bytesPerRow: 0,
            bitsPerPixel: 0
        ))
        bitmap.bitmapData?.initialize(
            repeating: 127,
            count: bitmap.bytesPerRow * bitmap.pixelsHigh
        )
        let bytes = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        try bytes.write(to: source)
        let artifactRoot = root.appendingPathComponent("artifacts", isDirectory: true)
        let store = MediaStudioInputArtifactStore(root: artifactRoot)

        let input = try await store.importImage(from: source)

        XCTAssertTrue(input.base64Data.isEmpty)
        let fileURL = try XCTUnwrap(input.fileURL)
        XCTAssertEqual(try Data(contentsOf: fileURL), bytes)
        XCTAssertNotEqual(fileURL, source)
        let permissions = try XCTUnwrap(
            FileManager.default.attributesOfItem(atPath: fileURL.path)[.posixPermissions]
                as? NSNumber
        )
        XCTAssertEqual(permissions.intValue & 0o777, 0o600)
        try FileManager.default.removeItem(at: source)
        XCTAssertEqual(try Data(contentsOf: fileURL), bytes)

        store.removeIfManaged(fileURL)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fileURL.path))
    }

    func testAudioSelectionUsesPrivateFileInsteadOfBase64() async throws {
        let root = try directory()
        let source = root.appendingPathComponent("rhythm.mp3")
        let bytes = Data((0..<64_000).map { UInt8($0 % 251) })
        try bytes.write(to: source)
        let store = MediaStudioInputArtifactStore(
            root: root.appendingPathComponent("artifacts", isDirectory: true)
        )

        let input = try await store.importAudio(from: source)

        XCTAssertTrue(input.base64Data.isEmpty)
        let fileURL = try XCTUnwrap(input.fileURL)
        XCTAssertEqual(input.mimeType, "audio/mpeg")
        XCTAssertEqual(try Data(contentsOf: fileURL), bytes)
        XCTAssertNotEqual(fileURL, source)
        store.removeAll()
        XCTAssertFalse(FileManager.default.fileExists(atPath: fileURL.path))
    }
}
