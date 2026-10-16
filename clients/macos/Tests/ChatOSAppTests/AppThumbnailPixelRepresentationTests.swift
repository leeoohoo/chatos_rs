import AppKit
import ImageIO
@testable import ChatOSApp
import XCTest

@MainActor
final class AppThumbnailPixelRepresentationTests: XCTestCase {
    func testDrawingAtHighDPIKeepsTheDecodedPixelBound() throws {
        let decoded = try XCTUnwrap(AppImageThumbnailLoader.decode(
            try png(), maximumSourcePixelCount: 128 * 64, maximumDisplayPixelSize: 24
        ))
        let image = decoded.makeNSImage()
        try assertBitmap(image, width: decoded.image.width, height: decoded.image.height)
        for scale in [1, 2, 3] {
            let target = try XCTUnwrap(NSBitmapImageRep(
                bitmapDataPlanes: nil,
                pixelsWide: decoded.image.width * scale, pixelsHigh: decoded.image.height * scale,
                bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
            ))
            let context = try XCTUnwrap(NSGraphicsContext(bitmapImageRep: target))
            NSGraphicsContext.saveGraphicsState()
            NSGraphicsContext.current = context
            context.cgContext.scaleBy(x: CGFloat(scale), y: CGFloat(scale))
            image.draw(in: NSRect(origin: .zero, size: image.size))
            NSGraphicsContext.restoreGraphicsState()
            try assertBitmap(image, width: decoded.image.width, height: decoded.image.height)
        }
        let data = try XCTUnwrap(image.tiffRepresentation)
        let source = try XCTUnwrap(CGImageSourceCreateWithData(data as CFData, nil))
        let roundTrip = try XCTUnwrap(CGImageSourceCreateImageAtIndex(source, 0, nil))
        XCTAssertEqual(roundTrip.width, decoded.image.width)
        XCTAssertEqual(roundTrip.height, decoded.image.height)
    }

    func testLocalFileCacheStoresOneBoundedBitmapAndReusesIt() async throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("thumbnail-pixel-representation-\(UUID().uuidString).png")
        defer { try? FileManager.default.removeItem(at: url) }
        try png().write(to: url)
        let loaded = await AppLocalImageThumbnailCache.image(
            for: url, maximumBytes: 1_024 * 1_024,
            maximumSourcePixelCount: 128 * 64, maximumDisplayPixelSize: 24
        )
        let image = try XCTUnwrap(loaded)
        let reused = await AppLocalImageThumbnailCache.image(
            for: url, maximumBytes: 1_024 * 1_024,
            maximumSourcePixelCount: 128 * 64, maximumDisplayPixelSize: 24
        )
        XCTAssertTrue(image === reused)
        try assertBitmap(image, width: 24, height: 12)
    }

    func testDataAndBase64CachesUseTheSamePixelBoundedRepresentation() async throws {
        let data = try png()
        let identity = UUID().uuidString
        let directResult = await AppImageDataThumbnailCache.image(
            for: data,
            cacheKey: AppImageDataThumbnailCache.dataIdentity(
                data, identity: identity, maximumSourcePixelCount: 128 * 64, maximumDisplayPixelSize: 24
            ), maximumBytes: data.count, maximumSourcePixelCount: 128 * 64, maximumDisplayPixelSize: 24
        )
        let direct = try XCTUnwrap(directResult)
        let base64 = data.base64EncodedString()
        let inlineResult = await AppImageDataThumbnailCache.image(
            forBase64: base64,
            cacheKey: AppImageDataThumbnailCache.base64Identity(
                base64, identity: identity, maximumSourcePixelCount: 128 * 64, maximumDisplayPixelSize: 24
            ), maximumEncodedCharacters: base64.count,
            maximumSourcePixelCount: 128 * 64, maximumDisplayPixelSize: 24
        )
        let inline = try XCTUnwrap(inlineResult)
        try assertBitmap(direct, width: 24, height: 12)
        try assertBitmap(inline, width: 24, height: 12)
    }

    private func assertBitmap(_ image: NSImage, width: Int, height: Int) throws {
        XCTAssertEqual(image.representations.count, 1)
        let bitmap = try XCTUnwrap(image.representations.first as? NSBitmapImageRep)
        XCTAssertEqual(bitmap.pixelsWide, width)
        XCTAssertEqual(bitmap.pixelsHigh, height)
        XCTAssertEqual(image.size, NSSize(width: width, height: height))
        XCTAssertEqual(bitmap.size, image.size)
    }

    private func png() throws -> Data {
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: 128, pixelsHigh: 64,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        return try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
    }
}
