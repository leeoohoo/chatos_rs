import AppKit
@testable import ChatOSApp
import XCTest

final class AgentAvatarImageProcessorTests: XCTestCase {
    func testNormalizeCropsAndProducesBoundedSquareJPEG() throws {
        let source = NSImage(size: NSSize(width: 640, height: 320))
        source.lockFocus()
        NSColor.systemPurple.setFill()
        NSRect(x: 0, y: 0, width: 640, height: 320).fill()
        source.unlockFocus()
        let sourceData = try XCTUnwrap(source.tiffRepresentation)

        let result = try AgentAvatarImageProcessor.normalize(sourceData)
        XCTAssertLessThanOrEqual(result.count, 512 * 1_024)
        let decoded = try XCTUnwrap(NSImage(data: result))
        let representation = try XCTUnwrap(decoded.representations.first)
        XCTAssertEqual(representation.pixelsWide, 256)
        XCTAssertEqual(representation.pixelsHigh, 256)
    }

    func testNormalizeRejectsInvalidData() {
        XCTAssertThrowsError(try AgentAvatarImageProcessor.normalize(Data("not an image".utf8)))
    }
}
