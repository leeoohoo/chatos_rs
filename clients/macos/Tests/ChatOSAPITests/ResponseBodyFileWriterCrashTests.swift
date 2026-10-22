@testable import ChatOSAPI
import Darwin
import Foundation
import XCTest

private struct ResponseBodyFileWriterCrashCheckpoint: Codable {
    var artifactName: String
    var payloadByteCount: Int
}

private enum ResponseBodyFileWriterCrashMode: String, CaseIterable, Sendable {
    case videoDownload
    case imageResponse
    case normalizedImage

    var artifactKind: ChatOSResponseBodyFileWriter.ArtifactKind {
        switch self {
        case .videoDownload: .videoDownload
        case .imageResponse: .imageResponse
        case .normalizedImage: .normalizedImage
        }
    }

    var artifactPrefix: String {
        switch self {
        case .videoDownload: ".chatos-video-download-"
        case .imageResponse: ".chatos-image-response-"
        case .normalizedImage: ".chatos-image-normalized-"
        }
    }
}

/// Selected explicitly in a disposable child xctest process. The normal suite has no probe
/// environment and returns immediately. The child keeps either an in-progress stream writer or
/// a completed file owner alive until the parent sends SIGKILL, so no Swift cleanup path can run.
final class ResponseBodyFileWriterCrashProbeTests: XCTestCase {
    func testCrashProbe() async throws {
        let environment = ProcessInfo.processInfo.environment
        guard let rootPath = environment["CHATOS_RESPONSE_BODY_CRASH_ROOT"],
              let markerPath = environment["CHATOS_RESPONSE_BODY_CRASH_MARKER"],
              let modeValue = environment["CHATOS_RESPONSE_BODY_CRASH_MODE"],
              let mode = ResponseBodyFileWriterCrashMode(rawValue: modeValue) else {
            return
        }
        let root = URL(fileURLWithPath: rootPath, isDirectory: true)
        let marker = URL(fileURLWithPath: markerPath)

        switch mode {
        case .videoDownload:
            try await runVideoDownloadProbe(root: root, marker: marker, mode: mode)
        case .imageResponse:
            try await runImageResponseProbe(root: root, marker: marker)
        case .normalizedImage:
            try await runNormalizedImageProbe(root: root, marker: marker, mode: mode)
        }
    }

    private func runVideoDownloadProbe(
        root: URL,
        marker: URL,
        mode: ResponseBodyFileWriterCrashMode
    ) async throws {
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
            ).first(where: { $0.lastPathComponent.hasPrefix(mode.artifactPrefix) }) {
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

    private func runImageResponseProbe(root: URL, marker: URL) async throws {
        let payload = Data((0..<400_000).map { UInt8($0 % 251) })
        let file = try await ChatOSBase64PayloadFileWriter.write(
            payload.base64EncodedString(),
            maximumBytes: payload.count,
            temporaryDirectory: root,
            availableCapacityOverride: Int64.max
        )
        let checkpoint = ResponseBodyFileWriterCrashCheckpoint(
            artifactName: file.url.deletingLastPathComponent().lastPathComponent,
            payloadByteCount: payload.count
        )
        try JSONEncoder().encode(checkpoint).write(to: marker, options: .atomic)

        withExtendedLifetime(file) {
            while true { _ = Darwin.pause() }
        }
    }

    private func runNormalizedImageProbe(
        root: URL,
        marker: URL,
        mode: ResponseBodyFileWriterCrashMode
    ) async throws {
        let payload = Data((0..<128_000).map { UInt8($0 % 251) })
        _ = try await ChatOSMediaArtifactFileWriter.writeNormalizedImage(
            maximumBytes: 1_024 * 1_024,
            temporaryDirectory: root,
            availableCapacityOverride: Int64.max
        ) { handle in
            try handle.write(contentsOf: payload)
            try handle.synchronize()
            let artifactURL = try XCTUnwrap(
                FileManager.default.contentsOfDirectory(
                    at: root,
                    includingPropertiesForKeys: nil
                ).first(where: { $0.lastPathComponent.hasPrefix(mode.artifactPrefix) })
            )
            let checkpoint = ResponseBodyFileWriterCrashCheckpoint(
                artifactName: artifactURL.lastPathComponent,
                payloadByteCount: payload.count
            )
            try JSONEncoder().encode(checkpoint).write(to: marker, options: .atomic)
            while true { _ = Darwin.pause() }
        }
    }
}

final class ResponseBodyFileWriterCrashTests: XCTestCase {
    func testVideoDownloadSIGKILLReleasesLeaseAndExpiredArtifactIsReconciled() async throws {
        try await verifySIGKILLRecovery(mode: .videoDownload)
    }

    func testImageResponseSIGKILLReleasesLeaseAndExpiredArtifactIsReconciled() async throws {
        try await verifySIGKILLRecovery(mode: .imageResponse)
    }

    func testNormalizedImageSIGKILLReleasesLeaseAndExpiredArtifactIsReconciled() async throws {
        try await verifySIGKILLRecovery(mode: .normalizedImage)
    }

    private func verifySIGKILLRecovery(mode: ResponseBodyFileWriterCrashMode) async throws {
        let base = FileManager.default.temporaryDirectory.appendingPathComponent(
            "ResponseBodyFileWriterCrashTests-\(mode.rawValue)-\(UUID().uuidString)",
            isDirectory: true
        )
        let root = base.appendingPathComponent("responses", isDirectory: true)
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
        environment["CHATOS_RESPONSE_BODY_CRASH_ROOT"] = root.path
        environment["CHATOS_RESPONSE_BODY_CRASH_MARKER"] = marker.path
        environment["CHATOS_RESPONSE_BODY_CRASH_MODE"] = mode.rawValue
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
                in: root,
                olderThan: Date(),
                maximumCandidates: 64,
                artifactKind: mode.artifactKind
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
                in: root,
                olderThan: Date(),
                maximumCandidates: 64,
                artifactKind: mode.artifactKind
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
