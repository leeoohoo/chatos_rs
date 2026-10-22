import Darwin
import Foundation
import XCTest
@testable import ChatOSApp

private enum MediaStudioVideoJobCrashMode: String, Codable, CaseIterable {
    case prepared
    case submittingWithoutProvider
    case providerKnown
    case finalizing
    case completed
}

private struct MediaStudioVideoJobCrashCheckpoint: Codable {
    var id: UUID
    var epoch: UUID
    var providerJobID: String?
    var status: MediaStudioVideoJobStore.Status
}

/// This test is selected explicitly in a disposable child xctest process. The normal suite
/// sees no probe environment and returns immediately. The child holds its execution lease
/// until the parent sends SIGKILL, so recovery exercises kernel lock release rather than a
/// Swift destructor or an ordinary thrown error.
final class MediaStudioVideoJobCrashProbeTests: XCTestCase {
    func testCrashProbe() async throws {
        let environment = ProcessInfo.processInfo.environment
        guard let rootPath = environment["CHATOS_MEDIA_JOB_CRASH_ROOT"],
              let markerPath = environment["CHATOS_MEDIA_JOB_CRASH_MARKER"],
              let modeValue = environment["CHATOS_MEDIA_JOB_CRASH_MODE"],
              let mode = MediaStudioVideoJobCrashMode(rawValue: modeValue) else {
            return
        }
        let owner = "media-crash-owner"
        let store = MediaStudioVideoJobStore(root: URL(fileURLWithPath: rootPath))
        let epoch = UUID()
        var execution: MediaStudioVideoJobStore.Execution? = try await store.prepare(
            request: .init(
                modelConfigID: "video-model",
                prompt: "SIGKILL checkpoint \(mode.rawValue)",
                size: "768P",
                seconds: 4
            ),
            owner: owner,
            sessionEpoch: epoch
        )
        let id = try XCTUnwrap(execution).snapshot.id
        let providerJobID = "provider-crash-\(mode.rawValue)"

        switch mode {
        case .prepared:
            break
        case .submittingWithoutProvider:
            execution = try await store.markSubmitting(
                try XCTUnwrap(execution),
                owner: owner,
                sessionEpoch: epoch
            )
        case .providerKnown:
            execution = try await store.markSubmitting(
                try XCTUnwrap(execution),
                owner: owner,
                sessionEpoch: epoch
            )
            try await store.recordProgress(
                id: id,
                owner: owner,
                sessionEpoch: epoch,
                progress: .init(status: "queued", jobID: providerJobID)
            )
        case .finalizing:
            execution = try await store.markSubmitting(
                try XCTUnwrap(execution),
                owner: owner,
                sessionEpoch: epoch
            )
            try await store.recordProgress(
                id: id,
                owner: owner,
                sessionEpoch: epoch,
                progress: .init(status: "completed", jobID: providerJobID)
            )
            try await store.markFinalizing(
                id: id,
                owner: owner,
                sessionEpoch: epoch,
                providerJobID: providerJobID
            )
        case .completed:
            execution = try await store.markSubmitting(
                try XCTUnwrap(execution),
                owner: owner,
                sessionEpoch: epoch
            )
            try await store.recordProgress(
                id: id,
                owner: owner,
                sessionEpoch: epoch,
                progress: .init(status: "completed", jobID: providerJobID)
            )
            try await store.markCompleted(
                id: id,
                owner: owner,
                sessionEpoch: epoch,
                providerJobID: providerJobID
            )
        }

        let snapshot = try await store.snapshot(id: id, owner: owner)
        let checkpoint = MediaStudioVideoJobCrashCheckpoint(
            id: id,
            epoch: epoch,
            providerJobID: snapshot.providerJobID,
            status: snapshot.status
        )
        try JSONEncoder().encode(checkpoint).write(
            to: URL(fileURLWithPath: markerPath),
            options: .atomic
        )

        withExtendedLifetime(execution) {
            while true { _ = Darwin.pause() }
        }
    }
}

final class MediaStudioVideoJobCrashTests: XCTestCase {
    private struct KilledCheckpoint {
        var root: URL
        var value: MediaStudioVideoJobCrashCheckpoint
    }

    private let owner = "media-crash-owner"

    func testPreparedCrashRecoversCreateWithoutChangingStableID() async throws {
        let killed = try await killProbe(at: .prepared)
        var recovered = try await MediaStudioVideoJobStore(root: killed.root).recover(
            owner: owner,
            sessionEpoch: UUID()
        )
        XCTAssertEqual(recovered?.snapshot.id, killed.value.id)
        XCTAssertEqual(recovered?.snapshot.status, .prepared)
        XCTAssertEqual(recovered?.mode, .create)
        recovered = nil
    }

    func testSubmittingCrashWithoutProviderBecomesReviewOnly() async throws {
        let killed = try await killProbe(at: .submittingWithoutProvider)
        let recovered = try await MediaStudioVideoJobStore(root: killed.root).recover(
            owner: owner,
            sessionEpoch: UUID()
        )
        XCTAssertEqual(recovered?.snapshot.id, killed.value.id)
        XCTAssertEqual(recovered?.snapshot.status, .submissionUncertain)
        XCTAssertEqual(recovered?.mode, .needsReview)
        XCTAssertNil(recovered?.snapshot.providerJobID)
    }

    func testProviderIdentityCrashResumesWithoutCreate() async throws {
        let killed = try await killProbe(at: .providerKnown)
        var recovered = try await MediaStudioVideoJobStore(root: killed.root).recover(
            owner: owner,
            sessionEpoch: UUID()
        )
        let providerJobID = try XCTUnwrap(killed.value.providerJobID)
        XCTAssertEqual(recovered?.snapshot.id, killed.value.id)
        XCTAssertEqual(recovered?.mode, .resume(providerJobID: providerJobID))
        XCTAssertEqual(recovered?.snapshot.providerJobID, providerJobID)
        recovered = nil
    }

    func testFinalizingCrashRetainsProviderIdentityForReconciliation() async throws {
        let killed = try await killProbe(at: .finalizing)
        var recovered = try await MediaStudioVideoJobStore(root: killed.root).recover(
            owner: owner,
            sessionEpoch: UUID()
        )
        let providerJobID = try XCTUnwrap(killed.value.providerJobID)
        XCTAssertEqual(killed.value.status, .finalizing)
        XCTAssertEqual(recovered?.snapshot.id, killed.value.id)
        XCTAssertEqual(recovered?.mode, .resume(providerJobID: providerJobID))
        recovered = nil
    }

    func testCompletedCrashDoesNotReopenProviderWork() async throws {
        let killed = try await killProbe(at: .completed)
        let store = MediaStudioVideoJobStore(root: killed.root)
        let recovered = try await store.recover(owner: owner, sessionEpoch: UUID())
        let snapshot = try await store.snapshot(id: killed.value.id, owner: owner)
        XCTAssertNil(recovered)
        XCTAssertEqual(snapshot.status, .completed)
        XCTAssertEqual(snapshot.providerJobID, killed.value.providerJobID)
    }

    private func killProbe(
        at mode: MediaStudioVideoJobCrashMode
    ) async throws -> KilledCheckpoint {
        let base = FileManager.default.temporaryDirectory.appendingPathComponent(
            "MediaStudioVideoJobCrashTests-\(UUID().uuidString)",
            isDirectory: true
        )
        let root = base.appendingPathComponent("jobs", isDirectory: true)
        let marker = base.appendingPathComponent("checkpoint.json")
        try FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: base) }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/xcrun")
        process.arguments = [
            "xctest",
            "-XCTest",
            "ChatOSAppTests.MediaStudioVideoJobCrashProbeTests/testCrashProbe",
            Bundle(for: MediaStudioVideoJobCrashProbeTests.self).bundleURL.path,
        ]
        var environment = ProcessInfo.processInfo.environment
        environment["CHATOS_MEDIA_JOB_CRASH_ROOT"] = root.path
        environment["CHATOS_MEDIA_JOB_CRASH_MARKER"] = marker.path
        environment["CHATOS_MEDIA_JOB_CRASH_MODE"] = mode.rawValue
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
                let stdout = String(
                    data: output.fileHandleForReading.readDataToEndOfFile(),
                    encoding: .utf8
                ) ?? ""
                let stderr = String(
                    data: errors.fileHandleForReading.readDataToEndOfFile(),
                    encoding: .utf8
                ) ?? ""
                throw NSError(
                    domain: "MediaStudioVideoJobCrashTests",
                    code: Int(process.terminationStatus),
                    userInfo: [NSLocalizedDescriptionKey: "Crash probe exited early.\n\(stdout)\n\(stderr)"]
                )
            }
            try await Task.sleep(for: .milliseconds(10))
        }
        guard FileManager.default.fileExists(atPath: marker.path) else {
            throw NSError(
                domain: "MediaStudioVideoJobCrashTests",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "Crash probe did not reach \(mode.rawValue)."]
            )
        }
        let checkpoint = try JSONDecoder().decode(
            MediaStudioVideoJobCrashCheckpoint.self,
            from: Data(contentsOf: marker)
        )

        if mode != .prepared, mode != .completed {
            do {
                _ = try await MediaStudioVideoJobStore(root: root).recover(
                    owner: owner,
                    sessionEpoch: UUID()
                )
                XCTFail("A live child execution lease must reject a competing recovery")
            } catch {
                XCTAssertEqual(
                    error.localizedDescription,
                    MediaStudioVideoJobStoreError.jobBusy.localizedDescription
                )
            }
        }

        XCTAssertEqual(Darwin.kill(process.processIdentifier, SIGKILL), 0)
        for _ in 0..<500 where process.isRunning {
            try await Task.sleep(for: .milliseconds(10))
        }
        guard !process.isRunning else {
            throw NSError(
                domain: "MediaStudioVideoJobCrashTests",
                code: 2,
                userInfo: [NSLocalizedDescriptionKey: "SIGKILL probe did not exit promptly."]
            )
        }
        mustTerminate = false
        XCTAssertEqual(process.terminationReason, .uncaughtSignal)
        XCTAssertEqual(process.terminationStatus, SIGKILL)
        return .init(root: root, value: checkpoint)
    }
}
