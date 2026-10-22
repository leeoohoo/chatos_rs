@testable import ChatOSAPI
import Darwin
import Foundation
import XCTest

private struct ResponseBodyFileWriterCrashCheckpoint: Codable {
    var artifactName: String
    var payloadByteCount: Int
}

/// Selected explicitly in a disposable child xctest process. The normal suite has no probe
/// environment and returns immediately. The child keeps the production writer suspended in
/// its response stream until the parent sends SIGKILL, so no Swift cleanup path can run.
final class ResponseBodyFileWriterCrashProbeTests: XCTestCase {
    func testCrashProbe() async throws {
        let environment = ProcessInfo.processInfo.environment
        guard let rootPath = environment["CHATOS_VIDEO_DOWNLOAD_CRASH_ROOT"],
              let markerPath = environment["CHATOS_VIDEO_DOWNLOAD_CRASH_MARKER"] else {
            return
        }
        let root = URL(fileURLWithPath: rootPath, isDirectory: true)
        let marker = URL(fileURLWithPath: markerPath)
        let payload = Data("partial-video-before-sigkill".utf8)
        var continuation: AsyncThrowingStream<Data, Error>.Continuation!
        let stream = AsyncThrowingStream<Data, Error> { continuation = $0 }
        let writing = Task {
            try await ChatOSResponseBodyFileWriter.write(
                stream,
                maximumBytes: 1_024,
                temporaryDirectory: root,
                availableCapacityOverride: Int64.max
            )
        }
        continuation.yield(payload)

        var artifact: URL?
        for _ in 0..<1_000 {
            if let candidate = try FileManager.default.contentsOfDirectory(
                at: root, includingPropertiesForKeys: nil
            ).first(where: { $0.lastPathComponent.hasPrefix(".chatos-video-download-") }) {
                let payloadURL = candidate.appendingPathComponent("payload.tmp")
                let size = try? payloadURL.resourceValues(forKeys: [.fileSizeKey]).fileSize
                if size == payload.count {
                    artifact = candidate
                    break
                }
            }
            try await Task.sleep(for: .milliseconds(10))
        }
        let artifactURL = try XCTUnwrap(artifact, "Production writer did not reach its checkpoint")
        let checkpoint = ResponseBodyFileWriterCrashCheckpoint(
            artifactName: artifactURL.lastPathComponent,
            payloadByteCount: payload.count
        )
        try JSONEncoder().encode(checkpoint).write(to: marker, options: .atomic)

        withExtendedLifetime(writing) {
            while true { _ = Darwin.pause() }
        }
    }
}

final class ResponseBodyFileWriterCrashTests: XCTestCase {
    func testSIGKILLReleasesLeaseAndExpiredArtifactIsReconciled() async throws {
        let base = FileManager.default.temporaryDirectory.appendingPathComponent(
            "ResponseBodyFileWriterCrashTests-\(UUID().uuidString)",
            isDirectory: true
        )
        let root = base.appendingPathComponent("downloads", isDirectory: true)
        let marker = base.appendingPathComponent("checkpoint.json")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: base) }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/xcrun")
        process.arguments = [
            "xctest",
            "-XCTest",
            "ChatOSAPITests.ResponseBodyFileWriterCrashProbeTests/testCrashProbe",
            Bundle(for: ResponseBodyFileWriterCrashProbeTests.self).bundleURL.path,
        ]
        var environment = ProcessInfo.processInfo.environment
        environment["CHATOS_VIDEO_DOWNLOAD_CRASH_ROOT"] = root.path
        environment["CHATOS_VIDEO_DOWNLOAD_CRASH_MARKER"] = marker.path
        process.environment = environment
        let output = Pipe()
        let errors = Pipe()
        process.standardOutput = output
        process.standardError = errors
        try process.run()
        var mustTerminate = true
        defer {
            if mustTerminate, process.isRunning {
                _ = Darwin.kill(process.processIdentifier, SIGKILL)
            }
        }

        for _ in 0..<1_000 {
            if FileManager.default.fileExists(atPath: marker.path) { break }
            if !process.isRunning {
                throw earlyExitError(process: process, output: output, errors: errors)
            }
            try await Task.sleep(for: .milliseconds(10))
        }
        guard FileManager.default.fileExists(atPath: marker.path) else {
            throw NSError(
                domain: "ResponseBodyFileWriterCrashTests",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "Crash probe did not reach its checkpoint."]
            )
        }
        let checkpoint = try JSONDecoder().decode(
            ResponseBodyFileWriterCrashCheckpoint.self,
            from: Data(contentsOf: marker)
        )
        let artifact = root.appendingPathComponent(checkpoint.artifactName, isDirectory: true)
        let payload = artifact.appendingPathComponent("payload.tmp")
        XCTAssertEqual(try Data(contentsOf: payload).count, checkpoint.payloadByteCount)

        // Treat the live artifact as old. The child's cross-process flock must still protect it.
        try FileManager.default.setAttributes(
            [.modificationDate: Date().addingTimeInterval(-48 * 60 * 60)],
            ofItemAtPath: artifact.path
        )
        XCTAssertEqual(
            ChatOSResponseBodyFileWriter.reconcileOrphanedArtifacts(
                in: root, olderThan: Date(), maximumCandidates: 64
            ),
            0
        )
        XCTAssertTrue(FileManager.default.fileExists(atPath: artifact.path))

        XCTAssertEqual(Darwin.kill(process.processIdentifier, SIGKILL), 0)
        for _ in 0..<500 where process.isRunning {
            try await Task.sleep(for: .milliseconds(10))
        }
        guard !process.isRunning else {
            throw NSError(
                domain: "ResponseBodyFileWriterCrashTests",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: "SIGKILL probe did not exit promptly."]
            )
        }
        mustTerminate = false
        XCTAssertEqual(process.terminationReason, .uncaughtSignal)
        XCTAssertEqual(process.terminationStatus, SIGKILL)

        XCTAssertEqual(
            ChatOSResponseBodyFileWriter.reconcileOrphanedArtifacts(
                in: root, olderThan: Date(), maximumCandidates: 64
            ),
            1
        )
        XCTAssertFalse(FileManager.default.fileExists(atPath: artifact.path))
    }

    private func earlyExitError(process: Process, output: Pipe, errors: Pipe) -> NSError {
        let stdout = String(
            data: output.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8
        ) ?? ""
        let stderr = String(
            data: errors.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8
        ) ?? ""
        return NSError(
            domain: "ResponseBodyFileWriterCrashTests",
            code: Int(process.terminationStatus),
            userInfo: [NSLocalizedDescriptionKey: "Crash probe exited early.\n\(stdout)\n\(stderr)"]
        )
    }
}
