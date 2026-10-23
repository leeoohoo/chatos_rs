import AppKit
import ChatOSAPI
import ChatOSCore
import ChatOSMediaWorkerClient
import ChatOSMediaWorkerProtocol
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class IndependentMediaWorkerConcurrencyTests: XCTestCase {
    func testIndependentMediaFeaturesEnterWorkersConcurrently() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "IndependentMediaWorkerConcurrencyTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let sourceURL = root.appendingPathComponent("source.png")
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
        try png.write(to: sourceURL)
        let worker = ConcurrentRecordingMediaWorker()

        let projectTask = Task {
            try await ProjectImagePreviewLoader.decode(
                content: "",
                isBinary: true,
                sourceFileURL: sourceURL,
                ownerUserID: "parallel-test",
                temporaryDirectory: root,
                availableCapacityOverride: Int64.max,
                mediaWorker: worker
            )
        }
        let studioTask = Task {
            try await MediaStudioImageLoader.thumbnail(
                for: .init(
                    id: "parallel-studio",
                    mimeType: "image/png",
                    url: sourceURL
                ),
                ownerUserID: "parallel-test",
                maximumDisplayPixelSize: 128,
                temporaryDirectory: root,
                availableCapacityOverride: Int64.max,
                mediaWorker: worker
            )
        }
        let avatarTask = Task {
            try await AgentAvatarImageLoader.normalizedData(
                forFile: sourceURL,
                ownerUserID: "parallel-test",
                temporaryDirectory: root,
                availableCapacityOverride: Int64.max,
                mediaWorker: worker
            )
        }
        defer {
            projectTask.cancel()
            studioTask.cancel()
            avatarTask.cancel()
        }

        let project = try await projectTask.value
        let studio = try await studioTask.value
        let avatar = try await avatarTask.value

        XCTAssertEqual(project?.image.width, 64)
        XCTAssertEqual(studio.image.width, 64)
        let avatarImage = try XCTUnwrap(NSBitmapImageRep(data: avatar))
        XCTAssertEqual(avatarImage.pixelsWide, 256)
        XCTAssertEqual(avatarImage.pixelsHigh, 256)
        let maximumConcurrentInvocationCount = await worker.maximumConcurrentInvocationCount()
        XCTAssertEqual(maximumConcurrentInvocationCount, 3)
    }

    func testIndependentMediaFeaturesRunThroughRealWorkersConcurrently() async throws {
        guard let path = ProcessInfo.processInfo.environment["CHATOS_TEST_MEDIA_WORKER_PATH"],
              !path.isEmpty else {
            throw XCTSkip("Set CHATOS_TEST_MEDIA_WORKER_PATH to run media concurrency integration.")
        }
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "IndependentRealMediaWorkerTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let sourceURL = root.appendingPathComponent("source.png")
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
        try XCTUnwrap(bitmap.representation(using: .png, properties: [:])).write(to: sourceURL)
        let worker = MediaWorkerProcessClient(
            executableURL: URL(fileURLWithPath: path),
            timeout: 10
        )

        async let project = ProjectImagePreviewLoader.decode(
            content: "",
            isBinary: true,
            sourceFileURL: sourceURL,
            ownerUserID: "parallel-process-test",
            temporaryDirectory: root,
            availableCapacityOverride: Int64.max,
            mediaWorker: worker
        )
        async let studio = MediaStudioImageLoader.thumbnail(
            for: .init(id: "parallel-process-studio", mimeType: "image/png", url: sourceURL),
            ownerUserID: "parallel-process-test",
            maximumDisplayPixelSize: 128,
            temporaryDirectory: root,
            availableCapacityOverride: Int64.max,
            mediaWorker: worker
        )
        async let avatar = AgentAvatarImageLoader.normalizedData(
            forFile: sourceURL,
            ownerUserID: "parallel-process-test",
            temporaryDirectory: root,
            availableCapacityOverride: Int64.max,
            mediaWorker: worker
        )

        let (projectResult, studioResult, avatarData) = try await (project, studio, avatar)
        XCTAssertEqual(projectResult?.image.width, 96)
        XCTAssertEqual(studioResult.image.width, 96)
        let avatarImage = try XCTUnwrap(NSBitmapImageRep(data: avatarData))
        XCTAssertEqual(avatarImage.pixelsWide, 256)
        XCTAssertEqual(avatarImage.pixelsHigh, 256)
        XCTAssertEqual(
            try FileManager.default.contentsOfDirectory(atPath: root.path),
            [sourceURL.lastPathComponent]
        )
    }
}

private actor ConcurrentRecordingMediaWorker: MediaWorkerImageNormalizing {
    private var activeInvocationCount = 0
    private var recordedMaximumConcurrentInvocationCount = 0

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
        _ = ownerUserID
        _ = maximumSourceBytes
        _ = maximumSourcePixelCount
        _ = maximumOutputPixelSize
        _ = maximumOutputBytes
        activeInvocationCount += 1
        recordedMaximumConcurrentInvocationCount = max(
            recordedMaximumConcurrentInvocationCount,
            activeInvocationCount
        )
        defer { activeInvocationCount -= 1 }
        try await Task.sleep(for: .milliseconds(100))

        let data = try Data(contentsOf: sourceURL)
        let bitmap = try XCTUnwrap(NSBitmapImageRep(data: data))
        let output = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let handle = try FileHandle(forWritingTo: destinationURL)
        try handle.truncate(atOffset: 0)
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

    func maximumConcurrentInvocationCount() -> Int {
        recordedMaximumConcurrentInvocationCount
    }
}
