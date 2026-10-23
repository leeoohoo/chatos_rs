import AppKit
import AVFoundation
import ChatOSCore
import ChatOSMediaWorkerClient
import CoreVideo
import Darwin
import Foundation
import XCTest
@testable import ChatOSApp

final class MediaWorkerProcessClientTests: XCTestCase {
    @MainActor
    func testBuiltWorkerExtractsLastVideoFrameOutOfProcess() async throws {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_TEST_MEDIA_WORKER_PATH"],
              !path.isEmpty else {
            throw XCTSkip("Set CHATOS_TEST_MEDIA_WORKER_PATH to run video-frame integration.")
        }
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let source = root.appendingPathComponent("source.mov")
        let destination = root.appendingPathComponent("frame.png")
        try await makeVideo(at: source)
        XCTAssertTrue(FileManager.default.createFile(
            atPath: destination.path,
            contents: nil,
            attributes: [.posixPermissions: 0o600]
        ))

        let result = try await MediaWorkerProcessClient(
            executableURL: URL(fileURLWithPath: path),
            timeout: 10
        ).extractLastVideoFramePNG(
            sourceURL: source,
            destinationURL: destination,
            ownerUserID: "process-client-test",
            maximumSourceBytes: 512 * 1_024 * 1_024,
            maximumOutputPixelSize: 4_096,
            maximumOutputBytes: 20 * 1_024 * 1_024
        )

        XCTAssertEqual(result.path, destination.path)
        XCTAssertEqual(result.pixelWidth, 64)
        XCTAssertEqual(result.pixelHeight, 32)
        XCTAssertNotNil(NSBitmapImageRep(data: try Data(contentsOf: destination)))
    }

    @MainActor
    func testStoryVideoTailExtractionReturnsLeaseOwnedWorkerArtifact() async throws {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_TEST_MEDIA_WORKER_PATH"],
              !path.isEmpty else {
            throw XCTSkip("Set CHATOS_TEST_MEDIA_WORKER_PATH to run Story tail integration.")
        }
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let source = root.appendingPathComponent("story.mov")
        try await makeVideo(at: source)

        var frame: GeneratedMediaFile? = try await StoryVideoFrameExtractor.lastFramePNG(
            from: source,
            ownerUserID: "process-client-test",
            temporaryDirectory: root,
            availableCapacityOverride: Int64.max,
            mediaWorker: MediaWorkerProcessClient(
                executableURL: URL(fileURLWithPath: path),
                timeout: 10
            )
        )
        let output = try XCTUnwrap(frame?.url)
        let artifact = output.deletingLastPathComponent()
        XCTAssertNotNil(NSBitmapImageRep(data: try Data(contentsOf: output)))
        XCTAssertTrue(FileManager.default.fileExists(atPath: artifact.path))
        frame = nil
        XCTAssertFalse(FileManager.default.fileExists(atPath: artifact.path))
    }

    func testMediaStudioThumbnailRunsThroughBuiltWorker() async throws {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_TEST_MEDIA_WORKER_PATH"],
              !path.isEmpty else {
            throw XCTSkip("Set CHATOS_TEST_MEDIA_WORKER_PATH to run thumbnail integration.")
        }
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil,
            pixelsWide: 128,
            pixelsHigh: 64,
            bitsPerSample: 8,
            samplesPerPixel: 4,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: .deviceRGB,
            bytesPerRow: 0,
            bitsPerPixel: 0
        ))
        let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))

        let thumbnail = try await MediaStudioImageLoader.thumbnail(
            for: .init(
                id: "real-worker-thumbnail",
                mimeType: "image/png",
                base64Data: png.base64EncodedString()
            ),
            ownerUserID: "process-client-test",
            maximumDisplayPixelSize: 32,
            temporaryDirectory: root,
            availableCapacityOverride: Int64.max,
            mediaWorker: MediaWorkerProcessClient(
                executableURL: URL(fileURLWithPath: path),
                timeout: 10
            )
        )

        XCTAssertEqual(thumbnail.image.width, 32)
        XCTAssertEqual(thumbnail.image.height, 16)
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty)
    }

    func testProjectImagePreviewSourceBackedRunsThroughBuiltWorkerWithoutCopy() async throws {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_TEST_MEDIA_WORKER_PATH"],
              !path.isEmpty else {
            throw XCTSkip("Set CHATOS_TEST_MEDIA_WORKER_PATH to run project-preview integration.")
        }
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil,
            pixelsWide: 96,
            pixelsHigh: 48,
            bitsPerSample: 8,
            samplesPerPixel: 4,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: .deviceRGB,
            bytesPerRow: 0,
            bitsPerPixel: 0
        ))
        let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let sourceURL = root.appendingPathComponent("project-preview-source.png")
        try png.write(to: sourceURL)

        let preview = try await ProjectImagePreviewLoader.decode(
            content: "",
            isBinary: true,
            sourceFileURL: sourceURL,
            ownerUserID: "process-client-test",
            temporaryDirectory: root,
            availableCapacityOverride: Int64.max,
            mediaWorker: MediaWorkerProcessClient(
                executableURL: URL(fileURLWithPath: path),
                timeout: 10
            )
        )

        XCTAssertEqual(preview?.image.width, 96)
        XCTAssertEqual(preview?.image.height, 48)
        XCTAssertEqual(
            try FileManager.default.contentsOfDirectory(atPath: root.path),
            [sourceURL.lastPathComponent]
        )
    }

    @MainActor
    func testPetFilePreviewSourceBackedRunsThroughBuiltWorkerWithoutCopy() async throws {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_TEST_MEDIA_WORKER_PATH"],
              !path.isEmpty else {
            throw XCTSkip("Set CHATOS_TEST_MEDIA_WORKER_PATH to run pet-preview integration.")
        }
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil,
            pixelsWide: 72,
            pixelsHigh: 36,
            bitsPerSample: 8,
            samplesPerPixel: 4,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: .deviceRGB,
            bytesPerRow: 0,
            bitsPerPixel: 0
        ))
        let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let sourceURL = root.appendingPathComponent("pet-preview-source.png")
        try png.write(to: sourceURL)
        let file = ProjectFileContent(
            path: sourceURL.path,
            displayPath: sourceURL.path,
            name: sourceURL.lastPathComponent,
            contentType: "image/png",
            isBinary: true,
            isWritable: true,
            size: Int64(png.count),
            modifiedAt: Date(),
            content: "",
            sourceFileURL: sourceURL
        )

        let preview = await PetFileImagePreviewCache.image(
            for: file,
            ownerUserID: "pet-process-client-test",
            temporaryDirectory: root,
            availableCapacityOverride: Int64.max,
            mediaWorker: MediaWorkerProcessClient(
                executableURL: URL(fileURLWithPath: path),
                timeout: 10
            )
        )

        XCTAssertEqual(preview?.representations.first?.pixelsWide, 72)
        XCTAssertEqual(preview?.representations.first?.pixelsHigh, 36)
        XCTAssertTrue(FileManager.default.fileExists(atPath: sourceURL.path))
        XCTAssertEqual(
            try FileManager.default.contentsOfDirectory(atPath: root.path),
            [sourceURL.lastPathComponent]
        )
    }

    func testAgentAvatarNormalizationRunsThroughBuiltWorker() async throws {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_TEST_MEDIA_WORKER_PATH"],
              !path.isEmpty else {
            throw XCTSkip("Set CHATOS_TEST_MEDIA_WORKER_PATH to run avatar integration.")
        }
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let source = root.appendingPathComponent("avatar-source.png")
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil,
            pixelsWide: 320,
            pixelsHigh: 160,
            bitsPerSample: 8,
            samplesPerPixel: 4,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: .deviceRGB,
            bytesPerRow: 0,
            bitsPerPixel: 0
        ))
        try XCTUnwrap(bitmap.representation(using: .png, properties: [:])).write(to: source)

        let avatar = try await AgentAvatarImageLoader.normalizedData(
            forFile: source,
            ownerUserID: "process-client-test",
            temporaryDirectory: root,
            availableCapacityOverride: Int64.max,
            mediaWorker: MediaWorkerProcessClient(
                executableURL: URL(fileURLWithPath: path),
                timeout: 10
            )
        )

        let decoded = try XCTUnwrap(NSBitmapImageRep(data: avatar))
        XCTAssertEqual(decoded.pixelsWide, 256)
        XCTAssertEqual(decoded.pixelsHigh, 256)
        XCTAssertEqual(
            try FileManager.default.contentsOfDirectory(atPath: root.path),
            [source.lastPathComponent]
        )
    }

    func testStoryNormalizationRunsThroughBuiltWorkerAndLeaseArtifact() async throws {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_TEST_MEDIA_WORKER_PATH"],
              !path.isEmpty else {
            throw XCTSkip("Set CHATOS_TEST_MEDIA_WORKER_PATH to run Story integration.")
        }
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil,
            pixelsWide: 48,
            pixelsHigh: 24,
            bitsPerSample: 8,
            samplesPerPixel: 4,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: .deviceRGB,
            bytesPerRow: 0,
            bitsPerPixel: 0
        ))
        let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        var file: GeneratedMediaFile? = try await MediaStudioImageLoader.normalizedPNGFile(
            for: .init(
                id: "real-media-worker",
                mimeType: "image/png",
                base64Data: png.base64EncodedString()
            ),
            ownerUserID: "process-client-test",
            temporaryDirectory: root,
            availableCapacityOverride: Int64.max,
            mediaWorker: MediaWorkerProcessClient(
                executableURL: URL(fileURLWithPath: path),
                timeout: 10
            )
        )
        let output = try XCTUnwrap(file?.url)
        let artifact = output.deletingLastPathComponent()
        let decoded = try XCTUnwrap(NSBitmapImageRep(data: Data(contentsOf: output)))
        XCTAssertEqual(decoded.pixelsWide, 48)
        XCTAssertEqual(decoded.pixelsHigh, 24)
        XCTAssertEqual(
            Set(try FileManager.default.contentsOfDirectory(atPath: root.path)),
            [artifact.lastPathComponent]
        )
        file = nil
        XCTAssertFalse(FileManager.default.fileExists(atPath: artifact.path))
    }

    func testBuiltWorkerCompletesHandshakeAndNormalizesOutOfProcess() async throws {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_TEST_MEDIA_WORKER_PATH"],
              !path.isEmpty else {
            throw XCTSkip("Set CHATOS_TEST_MEDIA_WORKER_PATH to run packaged-worker integration.")
        }
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let source = root.appendingPathComponent("source.png")
        let destination = root.appendingPathComponent("destination.png")
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil,
            pixelsWide: 32,
            pixelsHigh: 16,
            bitsPerSample: 8,
            samplesPerPixel: 4,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: .deviceRGB,
            bytesPerRow: 0,
            bitsPerPixel: 0
        ))
        try XCTUnwrap(bitmap.representation(using: .png, properties: [:])).write(to: source)
        XCTAssertTrue(FileManager.default.createFile(
            atPath: destination.path,
            contents: nil,
            attributes: [.posixPermissions: 0o600]
        ))

        let result = try await MediaWorkerProcessClient(
            executableURL: URL(fileURLWithPath: path),
            timeout: 10
        ).normalizePNG(
            sourceURL: source,
            destinationURL: destination,
            ownerUserID: "process-client-test",
            maximumSourceBytes: 20 * 1_024 * 1_024,
            maximumSourcePixelCount: 64_000_000,
            maximumOutputPixelSize: 8_192,
            maximumOutputBytes: 20 * 1_024 * 1_024
        )

        XCTAssertEqual(result.path, destination.path)
        XCTAssertEqual(result.pixelWidth, 32)
        XCTAssertEqual(result.pixelHeight, 16)
        XCTAssertGreaterThan(result.byteCount, 0)
    }

    func testBuiltWorkerNormalizesJPEGOutOfProcess() async throws {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_TEST_MEDIA_WORKER_PATH"],
              !path.isEmpty else {
            throw XCTSkip("Set CHATOS_TEST_MEDIA_WORKER_PATH to run packaged-worker integration.")
        }
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let source = root.appendingPathComponent("source.png")
        let destination = root.appendingPathComponent("destination.jpg")
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil,
            pixelsWide: 32,
            pixelsHigh: 16,
            bitsPerSample: 8,
            samplesPerPixel: 4,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: .deviceRGB,
            bytesPerRow: 0,
            bitsPerPixel: 0
        ))
        try XCTUnwrap(bitmap.representation(using: .png, properties: [:])).write(to: source)
        XCTAssertTrue(FileManager.default.createFile(
            atPath: destination.path,
            contents: nil,
            attributes: [.posixPermissions: 0o600]
        ))

        let result = try await MediaWorkerProcessClient(
            executableURL: URL(fileURLWithPath: path),
            timeout: 10
        ).normalizeJPEG(
            sourceURL: source,
            destinationURL: destination,
            ownerUserID: "process-client-test",
            maximumSourceBytes: 20 * 1_024 * 1_024,
            maximumSourcePixelCount: 64_000_000,
            maximumOutputPixelSize: 4_096,
            maximumOutputBytes: 20 * 1_024 * 1_024,
            compressionQuality: 0.82
        )

        let data = try Data(contentsOf: destination)
        XCTAssertEqual(result.mimeType, "image/jpeg")
        XCTAssertEqual(result.pixelWidth, 32)
        XCTAssertEqual(result.pixelHeight, 16)
        XCTAssertTrue(data.starts(with: [0xFF, 0xD8]))
        XCTAssertEqual(data.suffix(2), Data([0xFF, 0xD9]))
    }

    func testPastedTIFFNormalizationRunsThroughBuiltWorker() async throws {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_TEST_MEDIA_WORKER_PATH"],
              !path.isEmpty else {
            throw XCTSkip("Set CHATOS_TEST_MEDIA_WORKER_PATH to run pasted-image integration.")
        }
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
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
        let tiff = try XCTUnwrap(bitmap.representation(using: .tiff, properties: [:]))

        let output = try await AppPastedImageNormalizer.normalizeToPNGOffMain(
            tiff,
            ownerUserID: "process-client-test",
            temporaryDirectory: root,
            mediaWorker: MediaWorkerProcessClient(
                executableURL: URL(fileURLWithPath: path),
                timeout: 10
            )
        )

        XCTAssertTrue(output.starts(with: [0x89, 0x50, 0x4E, 0x47]))
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty)
    }

    func testFailedWorkerDoesNotDelayIndependentWorker() async throws {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_TEST_MEDIA_WORKER_PATH"],
              !path.isEmpty else {
            throw XCTSkip("Set CHATOS_TEST_MEDIA_WORKER_PATH to run worker isolation integration.")
        }
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let source = root.appendingPathComponent("source.png")
        let failedDestination = root.appendingPathComponent("failed.png")
        let healthyDestination = root.appendingPathComponent("healthy.png")
        let failingWorker = root.appendingPathComponent("failing-worker.sh")
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil,
            pixelsWide: 8,
            pixelsHigh: 8,
            bitsPerSample: 8,
            samplesPerPixel: 4,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: .deviceRGB,
            bytesPerRow: 0,
            bitsPerPixel: 0
        ))
        try XCTUnwrap(bitmap.representation(using: .png, properties: [:])).write(to: source)
        XCTAssertTrue(FileManager.default.createFile(atPath: failedDestination.path, contents: nil))
        XCTAssertTrue(FileManager.default.createFile(atPath: healthyDestination.path, contents: nil))
        try Data("#!/bin/sh\nexit 1\n".utf8).write(to: failingWorker)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: failingWorker.path
        )

        do {
            _ = try await MediaWorkerProcessClient(
                executableURL: failingWorker,
                timeout: 1
            ).normalizePNG(
                sourceURL: source,
                destinationURL: failedDestination,
                ownerUserID: "failed-worker-test",
                maximumSourceBytes: 1_024 * 1_024,
                maximumSourcePixelCount: 1_024,
                maximumOutputPixelSize: 32,
                maximumOutputBytes: 1_024 * 1_024
            )
            XCTFail("Failing worker unexpectedly returned an image")
        } catch {
            // The independent follow-up must not inherit this failure.
        }

        let started = ContinuousClock.now
        _ = try await MediaWorkerProcessClient(
            executableURL: URL(fileURLWithPath: path),
            timeout: 10
        ).normalizePNG(
            sourceURL: source,
            destinationURL: healthyDestination,
            ownerUserID: "healthy-worker-test",
            maximumSourceBytes: 1_024 * 1_024,
            maximumSourcePixelCount: 1_024,
            maximumOutputPixelSize: 32,
            maximumOutputBytes: 1_024 * 1_024
        )
        XCTAssertLessThan(started.duration(to: .now), .milliseconds(900))
    }

    func testCancellationTerminatesUnresponsiveWorker() async throws {
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let source = root.appendingPathComponent("source.png")
        let destination = root.appendingPathComponent("destination.png")
        let script = root.appendingPathComponent("hanging-worker.sh")
        let marker = root.appendingPathComponent("worker.pid")
        try Data("image".utf8).write(to: source)
        XCTAssertTrue(FileManager.default.createFile(atPath: destination.path, contents: nil))
        let scriptBody = "#!/bin/sh\nprintf '%s' \"$$\" > '\(marker.path)'\nexec /bin/sleep 60\n"
        try Data(scriptBody.utf8).write(to: script)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: script.path
        )

        let task = Task {
            try await MediaWorkerProcessClient(executableURL: script, timeout: 30).normalizePNG(
                sourceURL: source,
                destinationURL: destination,
                ownerUserID: "process-client-test",
                maximumSourceBytes: 1_024,
                maximumSourcePixelCount: 1_024,
                maximumOutputPixelSize: 32,
                maximumOutputBytes: 1_024
            )
        }
        for _ in 0..<1_000 where !FileManager.default.fileExists(atPath: marker.path) {
            try await Task.sleep(for: .milliseconds(5))
        }
        guard FileManager.default.fileExists(atPath: marker.path) else {
            task.cancel()
            _ = try? await task.value
            return XCTFail("Media worker did not launch before the test deadline")
        }
        let processID = try XCTUnwrap(Int32(
            XCTUnwrap(String(contentsOf: marker, encoding: .utf8))
        ))
        task.cancel()
        do {
            _ = try await task.value
            XCTFail("Cancelled media worker unexpectedly succeeded")
        } catch is CancellationError {
            // Expected.
        } catch {
            XCTFail("Unexpected cancellation error: \(error)")
        }
        for _ in 0..<200 {
            if kill(processID, 0) == -1, errno == ESRCH { return }
            try await Task.sleep(for: .milliseconds(5))
        }
        XCTFail("Cancelled media worker process remained alive")
    }

    func testTimeoutTerminatesUnresponsiveWorker() async throws {
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let source = root.appendingPathComponent("source.png")
        let destination = root.appendingPathComponent("destination.png")
        let script = root.appendingPathComponent("timeout-worker.sh")
        try Data("image".utf8).write(to: source)
        XCTAssertTrue(FileManager.default.createFile(atPath: destination.path, contents: nil))
        try Data("#!/bin/sh\nexec /bin/sleep 60\n".utf8).write(to: script)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: script.path
        )

        do {
            _ = try await MediaWorkerProcessClient(
                executableURL: script,
                timeout: 0.2
            ).normalizePNG(
                sourceURL: source,
                destinationURL: destination,
                ownerUserID: "process-client-test",
                maximumSourceBytes: 1_024,
                maximumSourcePixelCount: 1_024,
                maximumOutputPixelSize: 32,
                maximumOutputBytes: 1_024
            )
            XCTFail("Timed-out media worker unexpectedly succeeded")
        } catch MediaWorkerProcessClientError.timedOut {
            // The timeout branch owns process termination and waits for pipe EOF.
        }
    }

    func testWorkerExitsWhenDirectParentIsKilledEvenIfInputRemainsOpen() async throws {
        guard let workerPath = ProcessInfo.processInfo.environment["CHATOS_TEST_MEDIA_WORKER_PATH"],
              !workerPath.isEmpty else {
            throw XCTSkip("Set CHATOS_TEST_MEDIA_WORKER_PATH to run parent-death integration.")
        }
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let script = root.appendingPathComponent("parent-probe.sh")
        let fifo = root.appendingPathComponent("worker.stdin")
        let workerMarker = root.appendingPathComponent("worker.pid")
        let keeperMarker = root.appendingPathComponent("keeper.pid")
        let scriptBody = """
        #!/bin/sh
        /usr/bin/mkfifo '\(fifo.path)'
        ( /bin/sleep 60 ) > '\(fifo.path)' &
        keeper=$!
        '\(workerPath.replacingOccurrences(of: "'", with: "'\\''"))' --parent-pid "$$" < '\(fifo.path)' > /dev/null 2> /dev/null &
        worker=$!
        printf '%s' "$worker" > '\(workerMarker.path)'
        printf '%s' "$keeper" > '\(keeperMarker.path)'
        wait "$worker"
        """
        try Data(scriptBody.utf8).write(to: script)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: script.path
        )
        let parent = Process()
        parent.executableURL = script
        parent.standardInput = FileHandle.nullDevice
        parent.standardOutput = FileHandle.nullDevice
        parent.standardError = FileHandle.nullDevice
        try parent.run()
        var workerProcessID: Int32?
        var keeperProcessID: Int32?
        for _ in 0..<200 {
            if let worker = try? String(contentsOf: workerMarker, encoding: .utf8),
               let keeper = try? String(contentsOf: keeperMarker, encoding: .utf8),
               let parsedWorker = Int32(worker),
               let parsedKeeper = Int32(keeper) {
                workerProcessID = parsedWorker
                keeperProcessID = parsedKeeper
                break
            }
            try await Task.sleep(for: .milliseconds(5))
        }
        let worker = try XCTUnwrap(workerProcessID)
        let keeper = try XCTUnwrap(keeperProcessID)
        defer {
            _ = kill(worker, SIGKILL)
            _ = kill(keeper, SIGKILL)
            if parent.isRunning { _ = kill(parent.processIdentifier, SIGKILL) }
        }

        _ = kill(parent.processIdentifier, SIGKILL)
        for _ in 0..<200 {
            if kill(worker, 0) == -1, errno == ESRCH { return }
            try await Task.sleep(for: .milliseconds(5))
        }
        XCTFail("Media worker survived its direct parent's death")
    }

    private func makeRoot() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "MediaWorkerProcessClientTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root
    }

    @MainActor
    private func makeVideo(at url: URL) async throws {
        let writer = try AVAssetWriter(outputURL: url, fileType: .mov)
        let input = AVAssetWriterInput(
            mediaType: .video,
            outputSettings: [
                AVVideoCodecKey: AVVideoCodecType.h264,
                AVVideoWidthKey: 64,
                AVVideoHeightKey: 32,
            ]
        )
        input.expectsMediaDataInRealTime = false
        let adaptor = AVAssetWriterInputPixelBufferAdaptor(
            assetWriterInput: input,
            sourcePixelBufferAttributes: [
                kCVPixelBufferPixelFormatTypeKey as String:
                    kCVPixelFormatType_32BGRA,
                kCVPixelBufferWidthKey as String: 64,
                kCVPixelBufferHeightKey as String: 32,
            ]
        )
        guard writer.canAdd(input) else { throw CocoaError(.fileWriteUnknown) }
        writer.add(input)
        guard writer.startWriting() else {
            throw writer.error ?? CocoaError(.fileWriteUnknown)
        }
        writer.startSession(atSourceTime: .zero)
        for index in 0..<2 {
            while !input.isReadyForMoreMediaData {
                try await Task.sleep(for: .milliseconds(1))
            }
            var optionalBuffer: CVPixelBuffer?
            let status = CVPixelBufferCreate(
                kCFAllocatorDefault,
                64,
                32,
                kCVPixelFormatType_32BGRA,
                nil,
                &optionalBuffer
            )
            guard status == kCVReturnSuccess, let buffer = optionalBuffer else {
                throw CocoaError(.fileWriteUnknown)
            }
            CVPixelBufferLockBaseAddress(buffer, [])
            if let address = CVPixelBufferGetBaseAddress(buffer) {
                memset(address, index == 0 ? 40 : 180, CVPixelBufferGetDataSize(buffer))
            }
            CVPixelBufferUnlockBaseAddress(buffer, [])
            guard adaptor.append(
                buffer,
                withPresentationTime: CMTime(value: Int64(index), timescale: 30)
            ) else {
                throw writer.error ?? CocoaError(.fileWriteUnknown)
            }
        }
        input.markAsFinished()
        await writer.finishWriting()
        guard writer.status == .completed else {
            throw writer.error ?? CocoaError(.fileWriteUnknown)
        }
    }
}
