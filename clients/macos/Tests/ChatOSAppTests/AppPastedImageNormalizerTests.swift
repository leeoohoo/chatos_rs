import AppKit
import ChatOSCore
import ImageIO
@testable import ChatOSApp
import XCTest

final class AppPastedImageNormalizerTests: XCTestCase {
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
            historyStore: ConversationHistoryStore()
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
        let image = NSImage(size: NSSize(width: width, height: height))
        image.lockFocus()
        NSColor.systemPurple.setFill()
        NSRect(x: 0, y: 0, width: width, height: height).fill()
        image.unlockFocus()
        return try XCTUnwrap(image.tiffRepresentation)
    }
}
