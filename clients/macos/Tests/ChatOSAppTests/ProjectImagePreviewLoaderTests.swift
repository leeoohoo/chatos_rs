import AppKit
import ChatOSNetworking
import XCTest
@testable import ChatOSApp

final class ProjectImagePreviewLoaderTests: XCTestCase {
    func testProjectImagePreviewWaitsForSharedHeavyMediaLane() async throws {
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
        let governor = ClientHeavyMediaWorkGovernor(limit: 1)
        let blocker = try await governor.acquire(context: .init(
            taskID: "blocker",
            kind: .videoFrameExtraction,
            estimatedDecodedBytes: 64 * 1_024 * 1_024
        ))
        let preview = Task {
            try await ProjectImagePreviewLoader.decode(
                content: png.base64EncodedString(),
                isBinary: true,
                taskID: "project-image:test:preview",
                heavyMediaGovernor: governor
            )
        }
        try await waitForHeavyMediaQueue(governor)
        let queued = await governor.snapshot()
        XCTAssertEqual(queued.queued, [.init(
            taskID: "project-image:test:preview",
            kind: .imageDecode,
            estimatedDecodedBytes: 4_096 * 4_096 * 4
        )])

        await blocker.releaseAndWait()
        let result = try await preview.value
        let decoded = try XCTUnwrap(result)
        XCTAssertEqual(decoded.image.width, 64)
        XCTAssertEqual(decoded.image.height, 32)
        let completed = await governor.snapshot()
        XCTAssertTrue(completed.active.isEmpty)
        XCTAssertTrue(completed.queued.isEmpty)
    }

    func testProjectImageTaskIDDoesNotExposePath() {
        let path = "/Users/example/Secret Project/private-image.png"
        let first = ProjectImagePreviewLoader.taskID(
            path: path,
            size: 123,
            modifiedAt: Date(timeIntervalSince1970: 1_000)
        )
        let second = ProjectImagePreviewLoader.taskID(
            path: path,
            size: 123,
            modifiedAt: Date(timeIntervalSince1970: 1_000)
        )
        XCTAssertEqual(first, second)
        XCTAssertTrue(first.hasPrefix("project-image:"))
        XCTAssertFalse(first.contains("Secret Project"))
        XCTAssertFalse(first.contains("private-image.png"))
    }

    private func waitForHeavyMediaQueue(
        _ governor: ClientHeavyMediaWorkGovernor
    ) async throws {
        for _ in 0..<200 {
            if await governor.snapshot().queued.count == 1 { return }
            try await Task.sleep(for: .milliseconds(5))
        }
        XCTFail("Timed out waiting for heavy-media admission")
    }
}
