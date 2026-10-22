@testable import ChatOSAPI
import ChatOSCore
import Darwin
import Foundation
import XCTest

private enum UploadBodyArtifactCrashMode: String, Codable, CaseIterable {
    case videoJSON
    case imageMultipart
}

private struct UploadBodyArtifactCrashCheckpoint: Codable {
    var artifactName: String
    var payloadName: String
    var payloadByteCount: Int
}

/// Runs a production upload-body writer in an isolated xctest process and retains the returned
/// value until SIGKILL. The normal test suite has no probe environment and returns immediately.
final class UploadBodyArtifactCrashProbeTests: XCTestCase {
    func testCrashProbe() throws {
        let environment = ProcessInfo.processInfo.environment
        guard let rootPath = environment["CHATOS_UPLOAD_BODY_CRASH_ROOT"],
              let markerPath = environment["CHATOS_UPLOAD_BODY_CRASH_MARKER"],
              let modeValue = environment["CHATOS_UPLOAD_BODY_CRASH_MODE"],
              let mode = UploadBodyArtifactCrashMode(rawValue: modeValue) else {
            return
        }
        let root = URL(fileURLWithPath: rootPath, isDirectory: true)
        let marker = URL(fileURLWithPath: markerPath)
        let body: HTTPRequestBodyFile

        switch mode {
        case .videoJSON:
            let source = root.appendingPathComponent("source.mp4")
            try Data("video-before-sigkill".utf8).write(to: source)
            body = try ChatOSVideoRequestBodyFileWriter.write(
                payload: ["input": "__VIDEO__"],
                placeholder: "__VIDEO__",
                sourceURL: source,
                mimeType: "video/mp4",
                temporaryDirectory: root,
                availableCapacityOverride: Int64.max
            )
        case .imageMultipart:
            let source = root.appendingPathComponent("source.png")
            try Data("image-before-sigkill".utf8).write(to: source)
            body = try ChatOSImageMultipartBodyFileWriter.write(
                runtime: .init(
                    provider: "openai",
                    model: "gpt-image-1",
                    apiKey: "secret",
                    baseURL: "https://provider.example/v1",
                    enabled: true
                ),
                request: .init(
                    modelConfigID: "image",
                    prompt: "edit",
                    size: nil,
                    count: 1,
                    inputImage: .init(
                        name: "source.png", mimeType: "image/png", fileURL: source
                    )
                ),
                images: [
                    .init(name: "source.png", mimeType: "image/png", fileURL: source),
                ],
                temporaryDirectory: root,
                availableCapacityOverride: Int64.max
            ).bodyFile
        }

        let checkpoint = UploadBodyArtifactCrashCheckpoint(
            artifactName: body.url.deletingLastPathComponent().lastPathComponent,
            payloadName: body.url.lastPathComponent,
            payloadByteCount: body.byteCount
        )
        try JSONEncoder().encode(checkpoint).write(to: marker, options: .atomic)
        withExtendedLifetime(body) {
            while true { _ = Darwin.pause() }
        }
    }
}

final class UploadBodyArtifactCrashTests: XCTestCase {
    func testVideoJSONSIGKILLReleasesUploadLease() async throws {
        try await verifySIGKILLRecovery(mode: .videoJSON)
    }

    func testImageMultipartSIGKILLReleasesUploadLease() async throws {
        try await verifySIGKILLRecovery(mode: .imageMultipart)
    }

    private func verifySIGKILLRecovery(mode: UploadBodyArtifactCrashMode) async throws {
        let base = FileManager.default.temporaryDirectory.appendingPathComponent(
            "UploadBodyArtifactCrashTests-\(mode.rawValue)-\(UUID().uuidString)",
            isDirectory: true
        )
        let root = base.appendingPathComponent("requests", isDirectory: true)
        let marker = base.appendingPathComponent("checkpoint.json")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: base) }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/xcrun")
        process.arguments = [
            "xctest",
            "-XCTest",
            "ChatOSAPITests.UploadBodyArtifactCrashProbeTests/testCrashProbe",
            Bundle(for: UploadBodyArtifactCrashProbeTests.self).bundleURL.path,
        ]
        var environment = ProcessInfo.processInfo.environment
        environment["CHATOS_UPLOAD_BODY_CRASH_ROOT"] = root.path
        environment["CHATOS_UPLOAD_BODY_CRASH_MARKER"] = marker.path
        environment["CHATOS_UPLOAD_BODY_CRASH_MODE"] = mode.rawValue
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
                domain: "UploadBodyArtifactCrashTests",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "Crash probe did not reach \(mode.rawValue)."]
            )
        }
        let checkpoint = try JSONDecoder().decode(
            UploadBodyArtifactCrashCheckpoint.self,
            from: Data(contentsOf: marker)
        )
        let artifact = root.appendingPathComponent(checkpoint.artifactName, isDirectory: true)
        let payload = artifact.appendingPathComponent(checkpoint.payloadName)
        XCTAssertEqual(
            try payload.resourceValues(forKeys: [.fileSizeKey]).fileSize,
            checkpoint.payloadByteCount
        )

        try FileManager.default.setAttributes(
            [.modificationDate: Date().addingTimeInterval(-48 * 60 * 60)],
            ofItemAtPath: artifact.path
        )
        XCTAssertEqual(
            ChatOSUploadBodyArtifact.reconcileOrphans(
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
                domain: "UploadBodyArtifactCrashTests",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: "SIGKILL probe did not exit promptly."]
            )
        }
        mustTerminate = false
        XCTAssertEqual(process.terminationReason, .uncaughtSignal)
        XCTAssertEqual(process.terminationStatus, SIGKILL)
        XCTAssertEqual(
            ChatOSUploadBodyArtifact.reconcileOrphans(
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
            domain: "UploadBodyArtifactCrashTests",
            code: Int(process.terminationStatus),
            userInfo: [NSLocalizedDescriptionKey: "Crash probe exited early.\n\(stdout)\n\(stderr)"]
        )
    }
}
