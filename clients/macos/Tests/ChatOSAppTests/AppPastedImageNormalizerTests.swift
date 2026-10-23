import AppKit
import ChatOSCore
import ChatOSMediaWorkerClient
import ChatOSMediaWorkerProtocol
import ChatOSNetworking
import ImageIO
@testable import ChatOSApp
import XCTest

final class AppPastedImageNormalizerTests: XCTestCase {
    func testOffMainNormalizationRunsThroughMediaWorker() async throws {
        let limits = AppPastedImageNormalizationLimits(
            maximumInputBytes: 1_024 * 1_024,
            maximumSourcePixelCount: 128 * 64,
            maximumDecodePixelSize: 32,
            maximumOutputBytes: 1_024 * 1_024
        )
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "AppPastedImageNormalizerTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let worker = PastedImageRecordingWorker()
        let source = try makeTIFF(width: 128, height: 64)
        let output = try await AppPastedImageNormalizer.normalizeToPNGOffMain(
            source,
            limits: limits,
            ownerUserID: "test-user",
            temporaryDirectory: root,
            mediaWorker: worker
        )
        XCTAssertEqual(
            CGImageSourceGetType(try XCTUnwrap(
                CGImageSourceCreateWithData(output as CFData, nil)
            )) as String?,
            "public.png"
        )
        let callCount = await worker.callCount
        XCTAssertEqual(callCount, 1)
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty)
    }

    func testDirectUploadFormatsDoNotRequireNormalization() {
        XCTAssertFalse(AppPastedImageNormalizer.requiresPNGNormalization(mimeType: "image/png"))
        XCTAssertFalse(AppPastedImageNormalizer.requiresPNGNormalization(mimeType: "IMAGE/JPEG"))
        XCTAssertFalse(AppPastedImageNormalizer.requiresPNGNormalization(mimeType: "image/webp"))
        XCTAssertTrue(AppPastedImageNormalizer.requiresPNGNormalization(mimeType: "image/tiff"))
    }

    func testTIFFNormalizationProducesBoundedPNG() throws {
        let source = try makeTIFF(width: 128, height: 64)
        let limits = AppPastedImageNormalizationLimits(
            maximumInputBytes: 1_024 * 1_024,
            maximumSourcePixelCount: 128 * 64,
            maximumDecodePixelSize: 32,
            maximumOutputBytes: 1_024 * 1_024
        )

        let output = try AppPastedImageNormalizer.normalizeToPNG(source, limits: limits)
        let imageSource = try XCTUnwrap(CGImageSourceCreateWithData(output as CFData, nil))
        XCTAssertEqual(CGImageSourceGetType(imageSource) as String?, "public.png")
        let properties = try XCTUnwrap(
            CGImageSourceCopyPropertiesAtIndex(imageSource, 0, nil) as? [CFString: Any]
        )
        let width = try XCTUnwrap(properties[kCGImagePropertyPixelWidth] as? NSNumber).intValue
        let height = try XCTUnwrap(properties[kCGImagePropertyPixelHeight] as? NSNumber).intValue
        XCTAssertLessThanOrEqual(max(width, height), 32)
        XCTAssertGreaterThan(output.count, 0)
    }

    func testNormalizationRejectsInvalidDataAndPixelBombs() throws {
        XCTAssertThrowsError(
            try AppPastedImageNormalizer.normalizeToPNG(Data("not an image".utf8))
        )

        let source = try makeTIFF(width: 4, height: 4)
        let limits = AppPastedImageNormalizationLimits(
            maximumInputBytes: 1_024 * 1_024,
            maximumSourcePixelCount: 15,
            maximumDecodePixelSize: 4,
            maximumOutputBytes: 1_024 * 1_024
        )
        XCTAssertThrowsError(try AppPastedImageNormalizer.normalizeToPNG(source, limits: limits)) {
            guard case AppPastedImageNormalizationError.imageTooLarge = $0 else {
                return XCTFail("Expected pixel-budget rejection, got \($0)")
            }
        }
    }

    func testNormalizationRejectsOutputAboveByteBudget() throws {
        let source = try makeTIFF(width: 4, height: 4)
        let limits = AppPastedImageNormalizationLimits(
            maximumInputBytes: 1_024 * 1_024,
            maximumSourcePixelCount: 16,
            maximumDecodePixelSize: 4,
            maximumOutputBytes: 1
        )
        XCTAssertThrowsError(try AppPastedImageNormalizer.normalizeToPNG(source, limits: limits)) {
            guard case AppPastedImageNormalizationError.outputTooLarge = $0 else {
                return XCTFail("Expected output-budget rejection, got \($0)")
            }
        }
    }

    @MainActor
    func testConversationNormalizesTIFFBeforeAppendingAttachment() async throws {
        let viewModel = ConversationSessionViewModel(
            sessionID: "normalization-test",
            initialTurns: [],
            historyStore: ConversationHistoryStore(),
            pastedImageMediaWorker: PastedImageRecordingWorker()
        )
        viewModel.addPastedImage(
            data: try makeTIFF(width: 16, height: 8),
            mimeType: "image/tiff",
            suggestedName: "clipboard.tiff"
        )

        for _ in 0..<100 where viewModel.attachments.isEmpty {
            try await Task.sleep(for: .milliseconds(20))
        }
        let attachment = try XCTUnwrap(viewModel.attachments.first)
        XCTAssertEqual(attachment.mimeType, "image/png")
        XCTAssertEqual(attachment.name, "clipboard.png")
        XCTAssertEqual(
            CGImageSourceGetType(try XCTUnwrap(
                CGImageSourceCreateWithData(attachment.data as CFData, nil)
            )) as String?,
            "public.png"
        )
        XCTAssertTrue(viewModel.pastedImageNormalizationTasks.isEmpty)
    }

    private func makeTIFF(width: Int, height: Int) throws -> Data {
        // Pixel-budget fixtures must not depend on the current screen's
        // backing scale (lockFocus renders 2x pixels on Retina displays).
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        return try XCTUnwrap(bitmap.representation(using: .tiff, properties: [:]))
    }

}

private actor PastedImageRecordingWorker: MediaWorkerImageNormalizing {
    private(set) var callCount = 0

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
        callCount += 1
        let source = try Data(contentsOf: sourceURL)
        let output = try AppPastedImageNormalizer.normalizeToPNG(
            source,
            limits: .init(
                maximumInputBytes: maximumSourceBytes,
                maximumSourcePixelCount: maximumSourcePixelCount,
                maximumDecodePixelSize: maximumOutputPixelSize,
                maximumOutputBytes: maximumOutputBytes
            )
        )
        try output.write(to: destinationURL)
        return .init(
            path: destinationURL.path,
            mimeType: "image/png",
            byteCount: output.count,
            sha256: String(repeating: "0", count: 64),
            pixelWidth: 1,
            pixelHeight: 1
        )
    }
}
