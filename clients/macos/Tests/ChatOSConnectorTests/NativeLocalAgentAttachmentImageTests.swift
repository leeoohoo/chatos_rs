@testable import ChatOSConnector
import AppKit
import ChatOSCore
import ChatOSMediaWorkerClient
import ChatOSMediaWorkerProtocol
import CryptoKit
import Darwin
import Foundation
import ImageIO
import XCTest

final class NativeLocalAgentAttachmentImageTests: XCTestCase {
    func testModelImageUsesFileBackedWorkerAndCleansTemporaryOutput() async throws {
        let bytes = try imageBytes()
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "attachment-model-input-\(UUID().uuidString)", isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let sourceURL = root.appendingPathComponent("source.png")
        try bytes.write(to: sourceURL)
        let worker = AttachmentTestMediaWorker()
        let result = try await NativeLocalAgentAttachmentModelInput.result(
            source: try verifiedFile(sourceURL),
            record: .init(
                attachmentID: "attachment-a",
                conversationID: "conversation-a",
                turnID: "turn-a",
                messageID: "message-a",
                ordinal: 0,
                displayName: "image.png",
                mediaType: "image/png",
                byteSize: UInt64(bytes.count),
                sha256: SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined(),
                authorizedLocalRef: "attachment-ref",
                metadata: .object([:]),
                createdAtUnixMs: 0
            ),
            ownerUserID: "user-a",
            mediaWorker: worker
        )
        XCTAssertFalse(try suppliedImage(result).isEmpty)
        let calls = await worker.calls
        let call = try XCTUnwrap(calls.first)
        XCTAssertEqual(call.sourceURL, sourceURL)
        XCTAssertEqual(call.sourceIdentity, try verifiedFile(sourceURL).identity)
        XCTAssertFalse(FileManager.default.fileExists(atPath: call.destinationURL.path))
    }

    func testMainChatReadsCompleteImageWithoutTaskProjectOrPlugin() async throws {
        let bytes = try imageBytes()
        let fixture = try AttachmentDispatchFixture(bytes: bytes, name: "email.png", mediaType: "image/png")
        defer { fixture.cleanup() }
        let result = try await fixture.executor(
            mainChat: true,
            mediaWorker: AttachmentTestMediaWorker()
        ).execute(
            ownerUserID: "user-1", invocation: fixture.invocation(offset: 9, limit: 1, delivery: nil)
        )
        let image = try suppliedImage(result)
        XCTAssertFalse(image.isEmpty, "model delivery must not paginate or truncate image bytes")
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
        let result = try await fixture.executor(
            workspaceResolver: ImageTestWorkspaceRoot(root: root),
            mediaWorker: AttachmentTestMediaWorker()
        ).execute(
            ownerUserID: "user-1", invocation: fixture.invocation(delivery: "workspace")
        )
        let object = try fields(result)
        guard case let .string(path)? = object["workspace_relative_path"] else { return XCTFail("missing imported path") }
        XCTAssertEqual(try Data(contentsOf: root.appendingPathComponent(path)), bytes)
        XCTAssertFalse(try suppliedImage(result).isEmpty)
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
                _ = try await fixture.executor(mediaWorker: AttachmentTestMediaWorker()).execute(
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
        let result = try await fixture.executor(
            mainChat: true,
            mediaWorker: AttachmentTestMediaWorker()
        ).execute(
            ownerUserID: "user-1", invocation: fixture.invocation(delivery: "model")
        )
        let data = try suppliedImage(result)
        XCTAssertLessThanOrEqual(data.count, NativeLocalAgentAttachmentModelInput.maximumModelBytes)
        let source = try XCTUnwrap(CGImageSourceCreateWithData(data as CFData, nil))
        let image = try XCTUnwrap(CGImageSourceCreateImageAtIndex(source, 0, nil))
        XCTAssertLessThanOrEqual(image.width, 4_096)
        XCTAssertLessThan(try JSONEncoder().encode(result).count, LocalAgentHostFrameCodec.maximumFrameBytes)
    }

    func testMalformedAttachmentWorkerFailureDoesNotBlockAnotherConversation() async throws {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_TEST_MEDIA_WORKER_PATH"],
              !path.isEmpty else {
            throw XCTSkip("Set CHATOS_TEST_MEDIA_WORKER_PATH to run media-worker isolation integration.")
        }
        let valid = try AttachmentDispatchFixture(
            bytes: imageBytes(width: 512),
            name: "valid.png",
            mediaType: "image/png"
        )
        let malformed = try AttachmentDispatchFixture(
            bytes: Data(repeating: 0xff, count: 256 * 1_024),
            name: "malformed.png",
            mediaType: "image/png"
        )
        defer {
            valid.cleanup()
            malformed.cleanup()
        }
        let worker = MediaWorkerProcessClient(
            executableURL: URL(fileURLWithPath: path),
            timeout: 10
        )
        let validTask = Task {
            try await valid.executor(mainChat: true, mediaWorker: worker).execute(
                ownerUserID: "user-1",
                invocation: valid.invocation(delivery: "model")
            )
        }
        let malformedTask = Task {
            try await malformed.executor(mainChat: true, mediaWorker: worker).execute(
                ownerUserID: "user-1",
                invocation: malformed.invocation(delivery: "model")
            )
        }

        let validResult = try await validTask.value
        XCTAssertFalse(try suppliedImage(validResult).isEmpty)
        do {
            _ = try await malformedTask.value
            XCTFail("Malformed attachment unexpectedly succeeded")
        } catch {
            XCTAssertEqual(error as? NativeLocalAgentAttachmentModelInputError, .invalidImage)
        }
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

    private func verifiedFile(_ url: URL) throws -> NativeLocalAgentVerifiedAttachmentFile {
        var status = stat()
        guard url.path.withCString({ lstat($0, &status) }) == 0 else {
            throw FixtureError.invalid
        }
        return .init(
            url: url,
            identity: .init(
                device: UInt64(status.st_dev),
                inode: UInt64(status.st_ino),
                byteCount: Int64(status.st_size)
            )
        )
    }

    private enum FixtureError: Error { case invalid }
}

private actor AttachmentTestMediaWorker: MediaWorkerImageNormalizing {
    struct Call: Sendable {
        let sourceURL: URL
        let sourceIdentity: MediaWorkerFileIdentity?
        let destinationURL: URL
    }

    private(set) var calls: [Call] = []

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
        calls.append(.init(
            sourceURL: sourceURL,
            sourceIdentity: sourceIdentity,
            destinationURL: destinationURL
        ))
        let data = try Data(contentsOf: sourceURL)
        guard !ownerUserID.isEmpty,
              data.count <= maximumSourceBytes,
              let source = CGImageSourceCreateWithData(data as CFData, nil),
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = (properties[kCGImagePropertyPixelWidth] as? NSNumber)?.intValue,
              let height = (properties[kCGImagePropertyPixelHeight] as? NSNumber)?.intValue,
              width > 0,
              height > 0,
              width * height <= maximumSourcePixelCount,
              let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                  kCGImageSourceCreateThumbnailFromImageAlways: true,
                  kCGImageSourceCreateThumbnailWithTransform: true,
                  kCGImageSourceThumbnailMaxPixelSize: maximumOutputPixelSize,
              ] as CFDictionary),
              let png = NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:]) else {
            throw MediaWorkerProcessClientError.requestRejected(code: "invalid_image")
        }
        guard png.count <= maximumOutputBytes else {
            throw MediaWorkerProcessClientError.requestRejected(code: "output_too_large")
        }
        let handle = try FileHandle(forWritingTo: destinationURL)
        try handle.write(contentsOf: png)
        try handle.close()
        return .init(
            path: destinationURL.path,
            mimeType: "image/png",
            byteCount: png.count,
            sha256: SHA256.hash(data: png).map { String(format: "%02x", $0) }.joined(),
            pixelWidth: image.width,
            pixelHeight: image.height
        )
    }
}

private struct ImageTestWorkspaceRoot: NativeLocalAgentAttachmentWorkspaceResolving {
    let root: URL
    func attachmentWorkspaceRoot(ownerUserID: String, runID: String) async throws -> URL { root }
}
