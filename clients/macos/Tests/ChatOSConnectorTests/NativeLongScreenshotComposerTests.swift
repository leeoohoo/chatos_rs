import CoreGraphics
import Foundation
import Testing
@testable import ChatOSConnector

struct NativeLongScreenshotComposerTests {
    @Test
    func appendsOnlyNewRowsFromOverlappingFrames() async throws {
        let source = try makePatternImage(width: 96, height: 320)
        let first = try #require(source.cropping(to: CGRect(x: 0, y: 160, width: 96, height: 160)))
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
    }

    @Test
    func ignoresAnUnchangedFrame() async throws {
        let image = try makePatternImage(width: 80, height: 140)
        let composer = NativeLongScreenshotComposer()

        try await composer.start(with: image)
        let result = try await composer.append(image)

        #expect(result == .unchanged(totalHeight: 140))
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
}
