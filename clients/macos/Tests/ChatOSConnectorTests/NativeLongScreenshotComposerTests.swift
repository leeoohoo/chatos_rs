import CoreGraphics
import Foundation
import Testing
@testable import ChatOSConnector

struct NativeLongScreenshotComposerTests {
    @Test
    func appendsOnlyNewRowsFromOverlappingFrames() async throws {
        let source = try makePatternImage(width: 96, height: 320)
        let first = try #require(source.cropping(to: CGRect(x: 0, y: 0, width: 96, height: 160)))
        let second = try #require(source.cropping(to: CGRect(x: 0, y: 80, width: 96, height: 160)))
        let composer = NativeLongScreenshotComposer()

        try await composer.start(with: first)
        let result = try await composer.append(second)
        let output = try await composer.outputImage()

        guard case let .appended(newRows, totalHeight) = result else {
            Issue.record("Expected an overlapping frame to append, got \(result)")
            return
        }
        #expect(abs(newRows - 80) <= 2)
        #expect(abs(totalHeight - 240) <= 2)
        #expect(abs(output.height - 240) <= 2)
        let expected = try #require(source.cropping(to: CGRect(x: 0, y: 0, width: 96, height: 240)))
        #expect(try normalizedPixels(output) == normalizedPixels(expected))
    }

    @Test
    func ignoresAnUnchangedFrame() async throws {
        let image = try makePatternImage(width: 80, height: 140)
        let composer = NativeLongScreenshotComposer()

        try await composer.start(with: image)
        let result = try await composer.append(image)

        #expect(result == .unchanged(totalHeight: 140))
    }

    @Test
    func preservesTheInputOrientation() async throws {
        let image = try makePatternImage(width: 70, height: 130)
        let composer = NativeLongScreenshotComposer()

        try await composer.start(with: image)
        let output = try await composer.outputImage()

        let outputPixels = try normalizedPixels(output)
        let inputPixels = try normalizedPixels(image)
        #expect(outputPixels == inputPixels)
    }

    @Test
    func whiteDocumentDoesNotAppendAChangingOverlayAsNewContent() async throws {
        let source = try makeDocumentImage(width: 360, height: 1_000)
        let firstCrop = try #require(source.cropping(to: CGRect(x: 0, y: 0, width: 360, height: 600)))
        let scrolledCrop = try #require(source.cropping(to: CGRect(x: 0, y: 220, width: 360, height: 600)))
        let first = try addingOverlay(to: firstCrop, phase: 0)
        let overlayChanged = try addingOverlay(to: firstCrop, phase: 1)
        let scrolled = try addingOverlay(to: scrolledCrop, phase: 2)
        let composer = NativeLongScreenshotComposer()

        try await composer.start(with: first)
        let unchanged = try await composer.append(overlayChanged)
        let appended = try await composer.append(scrolled)

        #expect(unchanged == .unchanged(totalHeight: 600))
        guard case let .appended(newRows, totalHeight) = appended else {
            Issue.record("Expected the document scroll to append, got \(appended)")
            return
        }
        #expect(abs(newRows - 220) <= 2)
        #expect(abs(totalHeight - 820) <= 2)
    }

    @Test
    func repeatedOverlayUpdatesNeverIncreaseTheDocumentHeight() async throws {
        let source = try makeDocumentImage(width: 360, height: 800)
        let crop = try #require(source.cropping(to: CGRect(x: 0, y: 0, width: 360, height: 600)))
        let composer = NativeLongScreenshotComposer()

        try await composer.start(with: addingOverlay(to: crop, phase: 0))
        for phase in 1...6 {
            let result = try await composer.append(addingOverlay(to: crop, phase: phase % 3))
            #expect(result == .unchanged(totalHeight: 600))
        }
        #expect(await composer.currentHeight == 600)
    }

    @Test
    func sequentialDocumentScrollsAppendEachViewportOnce() async throws {
        let source = try makeDocumentImage(width: 360, height: 1_400)
        let composer = NativeLongScreenshotComposer()
        let first = try #require(source.cropping(to: CGRect(x: 0, y: 0, width: 360, height: 600)))
        try await composer.start(with: addingOverlay(to: first, phase: 0))

        for (index, offset) in [180, 360, 540].enumerated() {
            let crop = try #require(source.cropping(to: CGRect(
                x: 0,
                y: offset,
                width: 360,
                height: 600
            )))
            let result = try await composer.append(addingOverlay(to: crop, phase: (index + 1) % 3))
            guard case let .appended(newRows, totalHeight) = result else {
                Issue.record("Expected offset \(offset) to append once, got \(result)")
                return
            }
            #expect(abs(newRows - 180) <= 2)
            #expect(abs(totalHeight - (780 + index * 180)) <= 2)
        }
        #expect(await composer.currentHeight == 1_140)
    }

    private func makePatternImage(width: Int, height: Int) throws -> CGImage {
        var pixels = [UInt8](repeating: 255, count: width * height * 4)
        for y in 0..<height {
            for x in 0..<width {
                let index = (y * width + x) * 4
                pixels[index] = UInt8((y * 17 + x * 3) % 251)
                pixels[index + 1] = UInt8((y * 7 + x * 11) % 253)
                pixels[index + 2] = UInt8((y * 13 + x * 5) % 247)
                pixels[index + 3] = 255
            }
        }
        let data = Data(pixels) as CFData
        let provider = try #require(CGDataProvider(data: data))
        return try #require(CGImage(
            width: width,
            height: height,
            bitsPerComponent: 8,
            bitsPerPixel: 32,
            bytesPerRow: width * 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
            provider: provider,
            decode: nil,
            shouldInterpolate: false,
            intent: .defaultIntent
        ))
    }

    private func makeDocumentImage(width: Int, height: Int) throws -> CGImage {
        var pixels = [UInt8](repeating: 255, count: width * height * 4)
        for y in 0..<height {
            let pageY = y
            for x in 0..<width {
                let index = (y * width + x) * 4
                let isHeading = pageY % 260 < 7 && x > 22 && x < 230
                let isBodyLine = pageY % 34 < 3 && x > 30 && x < 300 - (pageY % 71)
                let isListMarker = pageY % 51 < 4 && x > 18 && x < 26
                if isHeading || isBodyLine || isListMarker {
                    pixels[index] = 35
                    pixels[index + 1] = 38
                    pixels[index + 2] = 42
                }
                pixels[index + 3] = 255
            }
        }

        let data = Data(pixels) as CFData
        let provider = try #require(CGDataProvider(data: data))
        return try #require(CGImage(
            width: width,
            height: height,
            bitsPerComponent: 8,
            bitsPerPixel: 32,
            bytesPerRow: width * 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
            provider: provider,
            decode: nil,
            shouldInterpolate: false,
            intent: .defaultIntent
        ))
    }

    private func addingOverlay(to image: CGImage, phase: Int) throws -> CGImage {
        let width = image.width
        let height = image.height
        guard let context = CGContext(
            data: nil,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: width * 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else {
            throw NativeLongScreenshotError.invalidImage
        }
        context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        context.setFillColor(CGColor(
            red: CGFloat(180 + phase * 20) / 255,
            green: CGFloat(80 + phase * 15) / 255,
            blue: CGFloat(30 + phase * 10) / 255,
            alpha: 1
        ))
        context.fill(CGRect(x: 305, y: 70, width: 45, height: 65))
        return try #require(context.makeImage())
    }

    private func normalizedPixels(_ image: CGImage) throws -> [UInt8] {
        let width = image.width
        let height = image.height
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let rendered = pixels.withUnsafeMutableBytes { bytes -> Bool in
            guard let baseAddress = bytes.baseAddress,
                  let context = CGContext(
                    data: baseAddress,
                    width: width,
                    height: height,
                    bitsPerComponent: 8,
                    bytesPerRow: width * 4,
                    space: CGColorSpaceCreateDeviceRGB(),
                    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
                  ) else { return false }
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        #expect(rendered)
        return pixels
    }
}
