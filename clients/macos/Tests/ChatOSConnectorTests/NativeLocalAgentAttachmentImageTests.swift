@testable import ChatOSConnector
import AppKit
import ChatOSCore
import ChatOSNetworking
import Foundation
import ImageIO
import XCTest

final class NativeLocalAgentAttachmentImageTests: XCTestCase {
    func testModelImageWaitsForSharedHeavyMediaLane() async throws {
        let bytes = try imageBytes()
        let governor = ClientHeavyMediaWorkGovernor(limit: 1)
        let blocker = try await governor.acquire(context: .init(
            taskID: "blocker",
            kind: .videoFrameExtraction,
            estimatedDecodedBytes: 64 * 1_024 * 1_024
        ))
        let normalization = Task {
            try await NativeLocalAgentAttachmentModelInput.result(
                bytes,
                record: .init(
                    attachmentID: "attachment-a",
                    conversationID: "conversation-a",
                    turnID: "turn-a",
                    messageID: "message-a",
                    ordinal: 0,
                    displayName: "image.png",
                    mediaType: "image/png",
                    byteSize: UInt64(bytes.count),
                    sha256: String(repeating: "a", count: 64),
                    authorizedLocalRef: "attachment-ref",
                    metadata: .object([:]),
                    createdAtUnixMs: 0
                ),
                taskID: "run-a",
                heavyMediaGovernor: governor
            )
        }
        try await waitForHeavyMediaQueue(governor)
        let snapshot = await governor.snapshot()
        XCTAssertEqual(snapshot.queued, [.init(
            taskID: "local-agent-image:run-a:attachment-a:model",
            kind: .imageDecode,
            estimatedDecodedBytes: 4_096 * 4_096 * 4
        )])

        await blocker.releaseAndWait()
        let result = try await normalization.value
        XCTAssertEqual(try suppliedImage(result), bytes)
        let completed = await governor.snapshot()
        XCTAssertTrue(completed.active.isEmpty)
    }

    func testMainChatReadsCompleteImageWithoutTaskProjectOrPlugin() async throws {
        let bytes = try imageBytes()
        let fixture = try AttachmentDispatchFixture(bytes: bytes, name: "email.png", mediaType: "image/png")
        defer { fixture.cleanup() }
        let result = try await fixture.executor(mainChat: true).execute(
            ownerUserID: "user-1", invocation: fixture.invocation(offset: 9, limit: 1, delivery: nil)
        )
        let image = try suppliedImage(result)
        XCTAssertEqual(image, bytes, "model delivery must not paginate or truncate image bytes")
        let fields = try fields(result)
        XCTAssertEqual(fields["sha256_verified"], .bool(true))
        XCTAssertEqual(fields["delivery"], .string("model"))
        XCTAssertNil(fields["workspace_relative_path"])
        guard case let .string(text)? = fields["text"] else { return XCTFail("missing text metadata") }
        XCTAssertFalse(text.contains("base64"))
        XCTAssertLessThan(text.count, 1_024)
        XCTAssertFalse(String(decoding: try JSONEncoder().encode(fields["_structured_result"]), as: UTF8.self).contains("base64"))
    }

    func testTaskWorkspaceImportAlsoSuppliesImageToModel() async throws {
        let bytes = try imageBytes()
        let fixture = try AttachmentDispatchFixture(bytes: bytes, name: "email.png", mediaType: "image/png")
        defer { fixture.cleanup() }
        let root = fixture.root.appendingPathComponent("workspace", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let result = try await fixture.executor(workspaceResolver: ImageTestWorkspaceRoot(root: root)).execute(
            ownerUserID: "user-1", invocation: fixture.invocation(delivery: "workspace")
        )
        let object = try fields(result)
        guard case let .string(path)? = object["workspace_relative_path"] else { return XCTFail("missing imported path") }
        XCTAssertEqual(try Data(contentsOf: root.appendingPathComponent(path)), bytes)
        XCTAssertEqual(try suppliedImage(result), bytes)
        XCTAssertEqual(object["sha256"], .string(fixture.spec.sha256))
    }

    func testMainChatCannotImportIntoAnExecutionWorkspace() async throws {
        let fixture = try AttachmentDispatchFixture()
        defer { fixture.cleanup() }
        do {
            _ = try await fixture.executor(mainChat: true).execute(
                ownerUserID: "user-1", invocation: fixture.invocation(delivery: "workspace")
            )
            XCTFail("Main Chat attachment reads must not create execution workspaces")
        } catch { XCTAssertEqual(error as? NativeLocalAgentPlatformToolError, .invalidArguments) }
    }

    func testModelDeliveryRejectsFakeImageAndNonImage() async throws {
        for mime in ["image/png", "application/octet-stream"] {
            let fixture = try AttachmentDispatchFixture(mediaType: mime)
            defer { fixture.cleanup() }
            do {
                _ = try await fixture.executor().execute(
                    ownerUserID: "user-1", invocation: fixture.invocation(delivery: "model")
                )
                XCTFail("a checksum or MIME label cannot substitute for valid image data")
            } catch {
                XCTAssertEqual(error as? NativeLocalAgentAttachmentModelInputError,
                               mime == "image/png" ? .invalidImage : .notImage)
            }
        }
    }

    func testModelDeliveryStillRejectsForeignOwner() async throws {
        let fixture = try AttachmentDispatchFixture(bytes: imageBytes(), mediaType: "image/png")
        defer { fixture.cleanup() }
        do {
            _ = try await fixture.executor(runOwner: "other-user", mainChat: true).execute(
                ownerUserID: "user-1", invocation: fixture.invocation(delivery: "model")
            )
            XCTFail("image delivery must retain account authorization")
        } catch { XCTAssertEqual(error as? NativeLocalAgentPlatformToolError, .invalidRunContext) }
    }

    func testLargeImageIsDecodedAndBoundedInsteadOfSilentlyDropped() async throws {
        // A wide, compressible fixture exercises resizing, without private content.
        let fixture = try AttachmentDispatchFixture(bytes: imageBytes(width: 5_000), mediaType: "image/png")
        defer { fixture.cleanup() }
        let result = try await fixture.executor(mainChat: true).execute(
            ownerUserID: "user-1", invocation: fixture.invocation(delivery: "model")
        )
        let data = try suppliedImage(result)
        XCTAssertLessThanOrEqual(data.count, NativeLocalAgentAttachmentModelInput.maximumModelBytes)
        let source = try XCTUnwrap(CGImageSourceCreateWithData(data as CFData, nil))
        let image = try XCTUnwrap(CGImageSourceCreateImageAtIndex(source, 0, nil))
        XCTAssertLessThanOrEqual(image.width, 4_096)
        XCTAssertLessThan(try JSONEncoder().encode(result).count, LocalAgentHostFrameCodec.maximumFrameBytes)
    }

    private func fields(_ result: LocalAgentJSONValue) throws -> [String: LocalAgentJSONValue] {
        guard case let .object(value) = result else { throw FixtureError.invalid }
        return value
    }

    private func suppliedImage(_ result: LocalAgentJSONValue) throws -> Data {
        let object = try fields(result)
        guard case let .array(images)? = object["_model_input"], images.count == 1,
              case let .object(image) = images[0], image["type"] == .string("input_image"),
              image["detail"] == .string("high"),
              case let .string(url)? = image["image_url"],
              let comma = url.firstIndex(of: ","), let bytes = Data(base64Encoded: String(url[url.index(after: comma)...])) else {
            throw FixtureError.invalid
        }
        XCTAssertNotNil(CGImageSourceCreateWithData(bytes as CFData, nil))
        return bytes
    }

    private func imageBytes(width: Int = 64) throws -> Data {
        let bitmap = try XCTUnwrap(NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: 32,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: width * 4, bitsPerPixel: 32))
        let pointer = try XCTUnwrap(bitmap.bitmapData)
        pointer.initialize(repeating: 200, count: width * 32 * 4)
        return try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
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

    private enum FixtureError: Error { case invalid }
}

private struct ImageTestWorkspaceRoot: NativeLocalAgentAttachmentWorkspaceResolving {
    let root: URL
    func attachmentWorkspaceRoot(ownerUserID: String, runID: String) async throws -> URL { root }
}
