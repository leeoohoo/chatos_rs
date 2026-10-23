import AppKit
import ChatOSAPI
import ChatOSCore
import ChatOSMediaWorkerProtocol
@testable import ChatOSApp
import XCTest

@MainActor
final class AgentAvatarImageProcessorTests: XCTestCase {
    func testNormalizePoliciesAreBounded() {
        XCTAssertEqual(AgentAvatarImageProcessor.maximumInputBytes, 20 * 1_024 * 1_024)
        XCTAssertEqual(AgentAvatarImageProcessor.maximumSourcePixelCount, 64_000_000)
        XCTAssertEqual(AgentAvatarImageProcessor.maximumDecodePixelSize, 2_048)
        XCTAssertEqual(AgentAvatarImageProcessor.outputPixelSize, 256)
        XCTAssertEqual(AgentAvatarImageProcessor.maximumOutputBytes, 512 * 1_024)
    }

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

    func testRemoteGeneratedAvatarStreamsThroughTemporaryFile() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "AgentAvatarRemoteImageTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: 320, pixelsHigh: 160,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        bitmap.bitmapData?.initialize(repeating: 110, count: bitmap.bytesPerRow * bitmap.pixelsHigh)
        let sourcePNG = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let transport = AvatarStreamingTransport(data: sourcePNG)
        let worker = AvatarRecordingMediaWorker()

        let result = try await AgentAvatarImageLoader.normalizedData(
            for: .init(
                id: "remote-avatar",
                mimeType: "image/png",
                url: URL(string: "https://cdn.example/avatar.png")!
            ),
            ownerUserID: "alice",
            transport: transport,
            temporaryDirectory: root,
            availableCapacityOverride: Int64.max,
            mediaWorker: worker
        )

        let decoded = try XCTUnwrap(NSBitmapImageRep(data: result))
        XCTAssertEqual(decoded.pixelsWide, 256)
        XCTAssertEqual(decoded.pixelsHigh, 256)
        XCTAssertLessThanOrEqual(result.count, AgentAvatarImageProcessor.maximumOutputBytes)
        let streamRequestCount = await transport.streamRequestCount
        let dataRequestCount = await transport.dataRequestCount
        XCTAssertEqual(streamRequestCount, 1)
        XCTAssertEqual(dataRequestCount, 0)
        let requests = await transport.requests
        XCTAssertEqual(requests.count, 1)
        XCTAssertNil(requests[0].headers["Authorization"])
        let invocation = await worker.invocation
        XCTAssertEqual(invocation?.ownerUserID, "alice")
        XCTAssertEqual(invocation?.maximumOutputPixelSize, 2_048)
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty)
    }
}

private actor AvatarRecordingMediaWorker: MediaWorkerImageNormalizing {
    struct Invocation: Sendable {
        let ownerUserID: String
        let maximumOutputPixelSize: Int
    }

    private(set) var invocation: Invocation?

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
        invocation = .init(
            ownerUserID: ownerUserID,
            maximumOutputPixelSize: maximumOutputPixelSize
        )
        let data = try Data(contentsOf: sourceURL)
        let bitmap = try XCTUnwrap(NSBitmapImageRep(data: data))
        let output = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let handle = try FileHandle(forWritingTo: destinationURL)
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
}

private actor AvatarStreamingTransport: HTTPTransport {
    let data: Data
    var requests: [HTTPRequest] = []
    var streamRequestCount = 0
    var dataRequestCount = 0

    init(data: Data) { self.data = data }

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        dataRequestCount += 1
        requests.append(request)
        return .init(statusCode: 200, headers: [:], body: data)
    }

    func stream(_ request: HTTPRequest) async throws -> HTTPStreamResponse {
        streamRequestCount += 1
        requests.append(request)
        let midpoint = data.count / 2
        return .init(
            statusCode: 200,
            headers: ["content-length": String(data.count)],
            body: AsyncThrowingStream { continuation in
                continuation.yield(Data(data.prefix(midpoint)))
                continuation.yield(Data(data.suffix(from: midpoint)))
                continuation.finish()
            }
        )
    }
}
