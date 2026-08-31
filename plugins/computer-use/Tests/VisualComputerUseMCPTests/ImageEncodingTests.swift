import CoreGraphics
import Foundation
import Testing
@testable import VisualComputerUseMCP

@Test func jpegValidatorRejectsPrematureMarkerInsideEntropyData() {
    let valid = Data([
        0xFF, 0xD8,
        0xFF, 0xE0, 0x00, 0x02,
        0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00,
        0x11, 0xFF, 0x00, 0x22,
        0xFF, 0xD9
    ])
    let malformed = Data([
        0xFF, 0xD8,
        0xFF, 0xE0, 0x00, 0x02,
        0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00,
        0x11, 0xFF, 0xE1, 0x22,
        0xFF, 0xD9
    ])

    #expect(ComputerController.isValidJPEGEncoding(valid))
    #expect(!ComputerController.isValidJPEGEncoding(malformed))
}

@Test func jpegValidatorRejectsTruncationAndTrailingBytes() {
    let truncated = Data([0xFF, 0xD8, 0xFF, 0xDA, 0x00, 0x02, 0x11])
    let trailing = Data([
        0xFF, 0xD8,
        0xFF, 0xDA, 0x00, 0x02,
        0x11, 0xFF, 0xD9, 0x00
    ])

    #expect(!ComputerController.isValidJPEGEncoding(truncated))
    #expect(!ComputerController.isValidJPEGEncoding(trailing))
}

@Test func repeatedJPEGEncodingAlwaysProducesValidIndependentData() throws {
    let width = 900
    let height = 900
    let bytesPerRow = width * 4
    let colorSpace = CGColorSpaceCreateDeviceRGB()
    let context = try #require(CGContext(
        data: nil,
        width: width,
        height: height,
        bitsPerComponent: 8,
        bytesPerRow: bytesPerRow,
        space: colorSpace,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    ))

    for row in 0..<45 {
        for column in 0..<45 {
            let red = CGFloat((row * 17 + column * 7) % 255) / 255
            let green = CGFloat((row * 11 + column * 19) % 255) / 255
            let blue = CGFloat((row * 23 + column * 13) % 255) / 255
            context.setFillColor(
                CGColor(
                    colorSpace: colorSpace,
                    components: [red, green, blue, 1]
                )!
            )
            context.fill(CGRect(
                x: column * 20,
                y: row * 20,
                width: 20,
                height: 20
            ))
        }
    }

    let image = try #require(context.makeImage())
    var priorData: Data?
    for _ in 0..<100 {
        let encoded = try #require(ComputerController.encodedData(
            from: image,
            format: .jpeg,
            jpegQuality: 0.82
        ))
        #expect(ComputerController.isValidJPEGEncoding(encoded))
        if let priorData {
            #expect(encoded == priorData)
        }
        priorData = encoded
    }
}
