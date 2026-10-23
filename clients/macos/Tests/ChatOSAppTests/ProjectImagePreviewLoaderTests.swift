import AppKit
import ChatOSMediaWorkerClient
import ChatOSMediaWorkerProtocol
import ChatOSNetworking
import XCTest
@testable import ChatOSApp

final class ProjectImagePreviewLoaderTests: XCTestCase {
    func testProjectImagePreviewMaterializesInlineSourceForWorker() async throws {
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil,
            pixelsWide: 64,
            pixelsHigh: 32,
            bitsPerSample: 8,
            samplesPerPixel: 4,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: .deviceRGB,
            bytesPerRow: 0,
            bitsPerPixel: 0
        ))
        let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let worker = ProjectPreviewRecordingWorker()
        let temporaryDirectory = FileManager.default.temporaryDirectory.appendingPathComponent(
            "ProjectImagePreviewLoaderTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(
            at: temporaryDirectory,
            withIntermediateDirectories: true
        )
        defer { try? FileManager.default.removeItem(at: temporaryDirectory) }
        let result = try await ProjectImagePreviewLoader.decode(
            content: png.base64EncodedString(),
            isBinary: true,
            ownerUserID: "alice",
            temporaryDirectory: temporaryDirectory,
            availableCapacityOverride: Int64.max,
            mediaWorker: worker
        )
        let decoded = try XCTUnwrap(result)
        XCTAssertEqual(decoded.image.width, 64)
        XCTAssertEqual(decoded.image.height, 32)
        let recordedInvocation = await worker.invocation()
        let invocation = try XCTUnwrap(recordedInvocation)
        XCTAssertEqual(invocation.ownerUserID, "alice")
        XCTAssertEqual(invocation.maximumOutputPixelSize, 4_096)
        XCTAssertFalse(FileManager.default.fileExists(atPath: invocation.sourceURL.path))
    }

    func testProjectImagePreviewPassesAuthorizedSourceFileDirectlyToWorker() async throws {
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil,
            pixelsWide: 80,
            pixelsHigh: 40,
            bitsPerSample: 8,
            samplesPerPixel: 4,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: .deviceRGB,
            bytesPerRow: 0,
            bitsPerPixel: 0
        ))
        let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "ProjectImageDirectSourceTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let sourceURL = root.appendingPathComponent("preview.png")
        try png.write(to: sourceURL)
        let worker = ProjectPreviewRecordingWorker()

        let preview = try await ProjectImagePreviewLoader.decode(
            content: "",
            isBinary: true,
            sourceFileURL: sourceURL,
            ownerUserID: "alice",
            temporaryDirectory: root,
            availableCapacityOverride: Int64.max,
            mediaWorker: worker
        )

        XCTAssertEqual(preview?.image.width, 80)
        XCTAssertEqual(preview?.image.height, 40)
        let invocation = await worker.invocation()
        XCTAssertEqual(invocation?.sourceURL, sourceURL)
        XCTAssertTrue(FileManager.default.fileExists(atPath: sourceURL.path))
        XCTAssertEqual(
            try FileManager.default.contentsOfDirectory(atPath: root.path),
            [sourceURL.lastPathComponent]
        )
    }

}

private actor ProjectPreviewRecordingWorker: MediaWorkerImageNormalizing {
    struct Invocation: Sendable {
        let sourceURL: URL
        let ownerUserID: String
        let maximumOutputPixelSize: Int
    }

    private var recordedInvocation: Invocation?

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
        _ = sourceIdentity
        _ = maximumSourceBytes
        _ = maximumSourcePixelCount
        _ = maximumOutputBytes
        recordedInvocation = .init(
            sourceURL: sourceURL,
            ownerUserID: ownerUserID,
            maximumOutputPixelSize: maximumOutputPixelSize
        )
        let data = try Data(contentsOf: sourceURL)
        let bitmap = try XCTUnwrap(NSBitmapImageRep(data: data))
        let output = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let handle = try FileHandle(forWritingTo: destinationURL)
        try handle.truncate(atOffset: 0)
        try handle.write(contentsOf: output)
        try handle.synchronize()
        try handle.close()
        return .init(
            path: destinationURL.path,
            mimeType: "image/png",
            byteCount: output.count,
            sha256: String(repeating: "0", count: 64),
            pixelWidth: bitmap.pixelsWide,
            pixelHeight: bitmap.pixelsHigh
        )
    }

    func invocation() -> Invocation? { recordedInvocation }
}
