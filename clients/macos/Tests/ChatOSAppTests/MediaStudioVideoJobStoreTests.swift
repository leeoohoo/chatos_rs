import ChatOSCore
import CryptoKit
import Darwin
import Foundation
import XCTest
@testable import ChatOSApp

final class MediaStudioVideoJobStoreTests: XCTestCase {
    private func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(
            "MediaStudioVideoJobStoreTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url
    }

    private func request(prompt: String = "durable") -> VideoGenerationRequest {
        .init(
            modelConfigID: "video-model",
            prompt: prompt,
            size: "768P",
            seconds: 4
        )
    }

    private func files(named name: String, under root: URL) -> [URL] {
        guard let enumerator = FileManager.default.enumerator(
            at: root,
            includingPropertiesForKeys: [.isRegularFileKey]
        ) else { return [] }
        return enumerator.compactMap { $0 as? URL }.filter { $0.lastPathComponent == name }
    }

    private func ownerDirectory(root: URL, owner: String) -> URL {
        let key = SHA256.hash(data: Data(owner.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
        return root.appendingPathComponent("Accounts/\(key)", isDirectory: true)
    }

    @discardableResult
    private func makeStagingDirectory(
        root: URL,
        owner: String = "alice",
        id: UUID = UUID(),
        modifiedAt: Date,
        hasLease: Bool = true
    ) throws -> URL {
        let folder = ownerDirectory(root: root, owner: owner).appendingPathComponent(
            ".staging-\(id.uuidString.lowercased())",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        if hasLease {
            try Data().write(to: folder.appendingPathComponent(".staging.lease"))
        }
        try FileManager.default.setAttributes(
            [.modificationDate: modifiedAt],
            ofItemAtPath: folder.path
        )
        return folder
    }

    func testWriterLockIsPrivateRegularFile() async throws {
        let root = try directory().appendingPathComponent("jobs")
        let store = MediaStudioVideoJobStore(root: root)

        _ = try await store.prepare(
            request: request(),
            owner: "alice",
            sessionEpoch: UUID()
        )

        let lockURL = root.appendingPathComponent(".writer.lock")
        let values = try lockURL.resourceValues(forKeys: [.isRegularFileKey])
        let attributes = try FileManager.default.attributesOfItem(atPath: lockURL.path)
        XCTAssertEqual(values.isRegularFile, true)
        XCTAssertEqual((attributes[.posixPermissions] as? NSNumber)?.intValue, 0o600)
    }

    func testStagingCleanupRemovesOnlyExpiredLeaseAwareDirectories() async throws {
        let directory = try directory()
        let root = directory.appendingPathComponent("jobs")
        let cutoff = Date(timeIntervalSinceNow: -60 * 60)
        let expired = try makeStagingDirectory(
            root: root,
            modifiedAt: cutoff.addingTimeInterval(-1)
        )
        let fresh = try makeStagingDirectory(
            root: root,
            modifiedAt: cutoff.addingTimeInterval(1)
        )
        let legacy = try makeStagingDirectory(
            root: root,
            modifiedAt: cutoff.addingTimeInterval(-1),
            hasLease: false
        )
        let ownerFolder = ownerDirectory(root: root, owner: "alice")
        let malformed = ownerFolder.appendingPathComponent(".staging-not-a-uuid", isDirectory: true)
        try FileManager.default.createDirectory(at: malformed, withIntermediateDirectories: true)
        let symlinkTarget = directory.appendingPathComponent("symlink-target", isDirectory: true)
        try FileManager.default.createDirectory(at: symlinkTarget, withIntermediateDirectories: true)
        let symlink = ownerFolder.appendingPathComponent(
            ".staging-\(UUID().uuidString.lowercased())",
            isDirectory: true
        )
        try FileManager.default.createSymbolicLink(at: symlink, withDestinationURL: symlinkTarget)

        let removed = try await MediaStudioVideoJobStore(root: root).removeExpiredStaging(
            owner: "alice",
            olderThan: cutoff
        )

        XCTAssertEqual(removed, 1)
        XCTAssertFalse(FileManager.default.fileExists(atPath: expired.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: fresh.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: legacy.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: malformed.path))
        XCTAssertNotNil(try? FileManager.default.destinationOfSymbolicLink(atPath: symlink.path))
    }

    func testStagingCleanupHonorsLiveLeaseUntilItIsReleased() async throws {
        let root = try directory().appendingPathComponent("jobs")
        let cutoff = Date(timeIntervalSinceNow: -60 * 60)
        let staging = try makeStagingDirectory(
            root: root,
            modifiedAt: cutoff.addingTimeInterval(-1)
        )
        let lease = staging.appendingPathComponent(".staging.lease")
        let descriptor = lease.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDWR | O_NOFOLLOW | O_CLOEXEC)
        }
        XCTAssertGreaterThanOrEqual(descriptor, 0)
        guard descriptor >= 0 else { return }
        XCTAssertEqual(flock(descriptor, LOCK_EX), 0)

        let store = MediaStudioVideoJobStore(root: root)
        let removedWhileLocked = try await store.removeExpiredStaging(
            owner: "alice",
            olderThan: cutoff
        )
        XCTAssertEqual(removedWhileLocked, 0)
        XCTAssertTrue(FileManager.default.fileExists(atPath: staging.path))

        XCTAssertEqual(flock(descriptor, LOCK_UN), 0)
        XCTAssertEqual(Darwin.close(descriptor), 0)
        let removedAfterRelease = try await store.removeExpiredStaging(
            owner: "alice",
            olderThan: cutoff
        )
        XCTAssertEqual(removedAfterRelease, 1)
        XCTAssertFalse(FileManager.default.fileExists(atPath: staging.path))
    }

    func testPrepareMaintainsExpiredStagingWithoutPublishingLeaseFile() async throws {
        let root = try directory().appendingPathComponent("jobs")
        let orphan = try makeStagingDirectory(
            root: root,
            modifiedAt: Date(timeIntervalSinceNow: -(25 * 60 * 60))
        )

        _ = try await MediaStudioVideoJobStore(root: root).prepare(
            request: request(),
            owner: "alice",
            sessionEpoch: UUID()
        )

        XCTAssertFalse(FileManager.default.fileExists(atPath: orphan.path))
        XCTAssertTrue(files(named: ".staging.lease", under: root).isEmpty)
    }

    func testStagingCleanupBoundsCandidatesPerPass() async throws {
        let root = try directory().appendingPathComponent("jobs")
        let cutoff = Date(timeIntervalSinceNow: -60 * 60)
        for _ in 0..<3 {
            try makeStagingDirectory(
                root: root,
                modifiedAt: cutoff.addingTimeInterval(-1)
            )
        }
        let store = MediaStudioVideoJobStore(root: root)

        let firstPass = try await store.removeExpiredStaging(
            owner: "alice",
            olderThan: cutoff,
            limit: 2
        )
        XCTAssertEqual(firstPass, 2)
        let ownerFolder = ownerDirectory(root: root, owner: "alice")
        XCTAssertEqual(
            try FileManager.default.contentsOfDirectory(atPath: ownerFolder.path)
                .filter { $0.hasPrefix(".staging-") }.count,
            1
        )
        let secondPass = try await store.removeExpiredStaging(
            owner: "alice",
            olderThan: cutoff,
            limit: 2
        )
        XCTAssertEqual(secondPass, 1)
    }

    func testStoryWorkloadIsInvisibleToGenericStudioRecovery() async throws {
        let root = try directory().appendingPathComponent("jobs")
        let store = MediaStudioVideoJobStore(root: root)
        let attemptID = UUID()
        let workload = MediaStudioVideoJobStore.Workload.storyVideo(
            projectID: UUID(),
            segmentID: "segment-1",
            attemptID: attemptID
        )
        let staged = try await store.stagePrepared(
            request: request(prompt: "story partition"),
            id: attemptID,
            owner: "alice",
            sessionEpoch: UUID(),
            workload: workload
        )
        XCTAssertEqual(staged.id, attemptID)
        XCTAssertEqual(staged.workload, workload)
        XCTAssertFalse(staged.isActive)

        let generic = try await store.recover(owner: "alice", sessionEpoch: UUID())
        XCTAssertNil(generic)
        let inactiveStory = try await store.recover(
            owner: "alice",
            sessionEpoch: UUID(),
            workload: workload
        )
        XCTAssertNil(inactiveStory)

        let activationEpoch = UUID()
        var activated: MediaStudioVideoJobStore.Execution? = try await store.activate(
            id: attemptID,
            owner: "alice",
            sessionEpoch: activationEpoch,
            workload: workload
        )
        XCTAssertTrue(try XCTUnwrap(activated).snapshot.isActive)
        XCTAssertEqual(try XCTUnwrap(activated).mode, .create)
        activated = nil

        let recovered = try await store.recover(
            owner: "alice",
            sessionEpoch: UUID(),
            workload: workload
        )
        XCTAssertEqual(recovered?.snapshot.id, attemptID)
        XCTAssertEqual(recovered?.snapshot.workload, workload)
        XCTAssertEqual(recovered?.mode, .create)
    }

    func testStableStoryStageIsIdempotentAndRejectsConflictingRequest() async throws {
        let directory = try directory()
        let root = directory.appendingPathComponent("jobs")
        let source = directory.appendingPathComponent("stable-input.png")
        let originalBytes = Data(repeating: 0x31, count: 4_096)
        try originalBytes.write(to: source)
        let store = MediaStudioVideoJobStore(root: root)
        let attemptID = UUID()
        let workload = MediaStudioVideoJobStore.Workload.storyVideo(
            projectID: UUID(),
            segmentID: "segment-1",
            attemptID: attemptID
        )
        let stableRequest = VideoGenerationRequest(
            modelConfigID: "video-model",
            prompt: "same intent",
            size: "768P",
            seconds: 4,
            inputImage: .init(
                name: "stable-input.png",
                mimeType: "image/png",
                fileURL: source
            )
        )

        let first = try await store.stagePrepared(
            request: stableRequest,
            id: attemptID,
            owner: "alice",
            sessionEpoch: UUID(),
            workload: workload
        )
        let repeated = try await MediaStudioVideoJobStore(root: root).stagePrepared(
            request: stableRequest,
            id: attemptID,
            owner: "alice",
            sessionEpoch: UUID(),
            workload: workload
        )
        XCTAssertEqual(repeated.id, first.id)
        XCTAssertEqual(repeated.idempotencyKey, first.idempotencyKey)
        XCTAssertFalse(repeated.isActive)

        try Data(repeating: 0x72, count: originalBytes.count).write(to: source)
        do {
            _ = try await store.stagePrepared(
                request: stableRequest,
                id: attemptID,
                owner: "alice",
                sessionEpoch: UUID(),
                workload: workload
            )
            XCTFail("A stable job ID must not be rebound to different request content")
        } catch {
            XCTAssertEqual(
                error.localizedDescription,
                MediaStudioVideoJobStoreError.jobConflict.localizedDescription
            )
        }

        do {
            _ = try await store.markSubmitting(
                id: attemptID,
                owner: "alice",
                sessionEpoch: first.sessionEpoch
            )
            XCTFail("A staged job must not execute before ownership activation")
        } catch {
            XCTAssertEqual(
                error.localizedDescription,
                MediaStudioVideoJobStoreError.activationRequired.localizedDescription
            )
        }
        XCTAssertEqual(files(named: "job.json", under: root).count, 1)

        let activationEpoch = UUID()
        let activated = try await store.activate(
            id: attemptID,
            owner: "alice",
            sessionEpoch: activationEpoch,
            workload: workload
        )
        XCTAssertEqual(
            try Data(contentsOf: XCTUnwrap(activated.request.inputImage?.fileURL)),
            originalBytes
        )
    }

    func testImportedProviderJobCanOnlyResumeAfterActivation() async throws {
        let root = try directory().appendingPathComponent("jobs")
        let source = try directory().appendingPathComponent("legacy.mov")
        try Data(repeating: 0x4A, count: 1_024).write(to: source)
        let store = MediaStudioVideoJobStore(root: root)
        let attemptID = UUID()
        let workload = MediaStudioVideoJobStore.Workload.storyVideo(
            projectID: UUID(),
            segmentID: "segment-legacy",
            attemptID: attemptID
        )
        let legacyRequest = VideoGenerationRequest(
            modelConfigID: "video-model",
            prompt: "legacy provider task",
            size: "768P",
            seconds: 4,
            referenceVideo: .init(
                name: "legacy.mov",
                mimeType: "video/quicktime",
                fileURL: source
            )
        )

        let staged = try await store.stageImportedProviderJob(
            request: legacyRequest,
            id: attemptID,
            providerJobID: "provider-existing",
            owner: "alice",
            sessionEpoch: UUID(),
            workload: workload
        )
        XCTAssertFalse(staged.isActive)
        XCTAssertEqual(staged.providerJobID, "provider-existing")
        XCTAssertTrue(files(named: "referenceVideo.artifact", under: root).isEmpty)
        let inactive = try await store.recover(
            owner: "alice",
            sessionEpoch: UUID(),
            workload: workload
        )
        XCTAssertNil(inactive)

        let activationEpoch = UUID()
        var activated: MediaStudioVideoJobStore.Execution? = try await store.activate(
            id: attemptID,
            owner: "alice",
            sessionEpoch: activationEpoch,
            workload: workload
        )
        XCTAssertEqual(
            try XCTUnwrap(activated).mode,
            .resume(providerJobID: "provider-existing")
        )
        XCTAssertNil(try XCTUnwrap(activated).request.referenceVideo)
        let activatedStateVersion = try XCTUnwrap(activated).snapshot.stateVersion
        activated = nil

        let repeatedActivation = try await store.activate(
            id: attemptID,
            owner: "alice",
            sessionEpoch: activationEpoch,
            workload: workload
        )
        XCTAssertEqual(repeatedActivation.mode, .resume(providerJobID: "provider-existing"))
        XCTAssertEqual(repeatedActivation.snapshot.stateVersion, activatedStateVersion)
    }

    func testVerifiedRetryReleaseTerminalizesStagedJobAndIsIdempotent() async throws {
        let root = try directory().appendingPathComponent("jobs")
        let store = MediaStudioVideoJobStore(root: root)
        let attemptID = UUID()
        let workload = MediaStudioVideoJobStore.Workload.storyVideo(
            projectID: UUID(),
            segmentID: "segment-release",
            attemptID: attemptID
        )
        _ = try await store.stagePrepared(
            request: request(prompt: "release staged task"),
            id: attemptID,
            owner: "alice",
            sessionEpoch: UUID(),
            workload: workload
        )
        let epoch = UUID()
        _ = try await store.activate(
            id: attemptID,
            owner: "alice",
            sessionEpoch: epoch,
            workload: workload
        )

        let released = try await store.releaseAfterVerifiedRetry(
            id: attemptID,
            owner: "alice",
            sessionEpoch: epoch,
            workload: workload
        )
        XCTAssertEqual(released.status, .cancelled)
        XCTAssertEqual(released.failurePhase, "verified_retry_release")
        XCTAssertFalse(released.retryable)
        XCTAssertTrue(files(named: "firstFrame.artifact", under: root).isEmpty)

        let repeated = try await store.releaseAfterVerifiedRetry(
            id: attemptID,
            owner: "alice",
            sessionEpoch: epoch,
            workload: workload
        )
        XCTAssertEqual(repeated, released)
        do {
            try await store.markCompleted(
                id: attemptID,
                owner: "alice",
                sessionEpoch: epoch,
                providerJobID: "late-provider-result"
            )
            XCTFail("A late provider result must not reopen an explicitly released job")
        } catch {
            XCTAssertEqual(
                error.localizedDescription,
                MediaStudioVideoJobStoreError.invalidTransition.localizedDescription
            )
        }
        let recovered = try await store.recover(
            owner: "alice",
            sessionEpoch: UUID(),
            workload: workload
        )
        XCTAssertNil(recovered)
    }

    func testVerifiedRetryReleaseRetainsProviderIdentityAndRejectsForeignWorkload() async throws {
        let root = try directory().appendingPathComponent("jobs")
        let store = MediaStudioVideoJobStore(root: root)
        let attemptID = UUID()
        let projectID = UUID()
        let workload = MediaStudioVideoJobStore.Workload.storyVideo(
            projectID: projectID,
            segmentID: "segment-provider",
            attemptID: attemptID
        )
        _ = try await store.stageImportedProviderJob(
            request: request(prompt: "retain provider identity"),
            id: attemptID,
            providerJobID: "provider-audit",
            owner: "alice",
            sessionEpoch: UUID(),
            workload: workload
        )
        let epoch = UUID()
        _ = try await store.activate(
            id: attemptID,
            owner: "alice",
            sessionEpoch: epoch,
            workload: workload
        )

        do {
            _ = try await store.releaseAfterVerifiedRetry(
                id: attemptID,
                owner: "alice",
                sessionEpoch: epoch,
                workload: .storyVideo(
                    projectID: projectID,
                    segmentID: "foreign-segment",
                    attemptID: attemptID
                )
            )
            XCTFail("A foreign Story association must not release this record")
        } catch {
            XCTAssertEqual(
                error.localizedDescription,
                MediaStudioVideoJobStoreError.jobConflict.localizedDescription
            )
        }

        let released = try await store.releaseAfterVerifiedRetry(
            id: attemptID,
            owner: "alice",
            sessionEpoch: epoch,
            workload: workload
        )
        XCTAssertEqual(released.status, .cancelled)
        XCTAssertEqual(released.providerJobID, "provider-audit")
    }

    func testSubmissionUncertainRequiresExplicitVerifiedRelease() async throws {
        let root = try directory().appendingPathComponent("jobs")
        let store = MediaStudioVideoJobStore(root: root)
        let attemptID = UUID()
        let workload = MediaStudioVideoJobStore.Workload.storyVideo(
            projectID: UUID(),
            segmentID: "segment-uncertain",
            attemptID: attemptID
        )
        _ = try await store.stagePrepared(
            request: request(prompt: "uncertain submission"),
            id: attemptID,
            owner: "alice",
            sessionEpoch: UUID(),
            workload: workload
        )
        let submittingEpoch = UUID()
        _ = try await store.activate(
            id: attemptID,
            owner: "alice",
            sessionEpoch: submittingEpoch,
            workload: workload
        )
        _ = try await store.markSubmitting(
            id: attemptID,
            owner: "alice",
            sessionEpoch: submittingEpoch
        )
        let reviewEpoch = UUID()
        let review = try await store.recover(
            owner: "alice",
            sessionEpoch: reviewEpoch,
            workload: workload
        )
        XCTAssertEqual(review?.snapshot.status, .submissionUncertain)
        XCTAssertEqual(review?.mode, .needsReview)

        let released = try await store.releaseAfterVerifiedRetry(
            id: attemptID,
            owner: "alice",
            sessionEpoch: reviewEpoch,
            workload: workload
        )
        XCTAssertEqual(released.status, .cancelled)
        XCTAssertEqual(released.failurePhase, "verified_retry_release")
    }

    func testLegacyBatchSubmissionWithoutProviderIDImportsAsReviewOnly() async throws {
        let directory = try directory()
        let root = directory.appendingPathComponent("jobs")
        let source = directory.appendingPathComponent("legacy-reference.mov")
        try Data(repeating: 0x42, count: 2_048).write(to: source)
        let store = MediaStudioVideoJobStore(root: root)
        let intentID = UUID()
        let workload = MediaStudioVideoJobStore.Workload.storyBatchVideo(
            projectID: UUID(),
            batchID: UUID(),
            segmentID: "segment-batch",
            attemptID: intentID
        )
        let legacyRequest = VideoGenerationRequest(
            modelConfigID: "video-model",
            prompt: "ambiguous legacy batch submission",
            size: "768P",
            seconds: 4,
            referenceVideo: .init(
                name: "legacy-reference.mov",
                mimeType: "video/quicktime",
                fileURL: source
            )
        )

        let staged = try await store.stageSubmissionUncertain(
            request: legacyRequest,
            id: intentID,
            owner: "alice",
            sessionEpoch: UUID(),
            workload: workload
        )
        let repeated = try await MediaStudioVideoJobStore(root: root).stageSubmissionUncertain(
            request: legacyRequest,
            id: intentID,
            owner: "alice",
            sessionEpoch: UUID(),
            workload: workload
        )
        XCTAssertEqual(repeated.id, staged.id)
        XCTAssertEqual(repeated.status, .submissionUncertain)
        XCTAssertFalse(repeated.isActive)
        XCTAssertTrue(files(named: "referenceVideo.artifact", under: root).isEmpty)
        let inactive = try await store.recover(
            owner: "alice",
            sessionEpoch: UUID(),
            workload: workload
        )
        XCTAssertNil(inactive)

        let activated = try await store.activate(
            id: intentID,
            owner: "alice",
            sessionEpoch: UUID(),
            workload: workload
        )
        XCTAssertEqual(activated.mode, .needsReview)
        XCTAssertNil(activated.snapshot.providerJobID)
    }

    func testProviderIdentityCannotChangeAfterFirstDurableObservation() async throws {
        let root = try directory().appendingPathComponent("jobs")
        let store = MediaStudioVideoJobStore(root: root)
        let session = UUID()
        let prepared = try await store.prepare(
            request: request(prompt: "provider identity"),
            owner: "alice",
            sessionEpoch: session
        )
        _ = try await store.markSubmitting(
            id: prepared.snapshot.id,
            owner: "alice",
            sessionEpoch: session
        )
        try await store.recordProgress(
            id: prepared.snapshot.id,
            owner: "alice",
            sessionEpoch: session,
            progress: .init(status: "queued", jobID: "provider-a")
        )

        do {
            try await store.recordProgress(
                id: prepared.snapshot.id,
                owner: "alice",
                sessionEpoch: session,
                progress: .init(status: "running", jobID: "provider-b")
            )
            XCTFail("A provider task must not be rebound after its identity is durable")
        } catch {
            XCTAssertEqual(
                error.localizedDescription,
                MediaStudioVideoJobStoreError.jobConflict.localizedDescription
            )
        }
        do {
            try await store.markFinalizing(
                id: prepared.snapshot.id,
                owner: "alice",
                sessionEpoch: session,
                providerJobID: "provider-b"
            )
            XCTFail("Finalization must retain the first provider identity")
        } catch {
            XCTAssertEqual(
                error.localizedDescription,
                MediaStudioVideoJobStoreError.jobConflict.localizedDescription
            )
        }
        let snapshot = try await store.snapshot(id: prepared.snapshot.id, owner: "alice")
        XCTAssertEqual(snapshot.providerJobID, "provider-a")
    }

    func testManifestWithoutWorkloadRemainsGenericStudioCompatible() async throws {
        let root = try directory().appendingPathComponent("jobs")
        let store = MediaStudioVideoJobStore(root: root)
        let prepared = try await store.prepare(
            request: request(prompt: "legacy generic"),
            owner: "alice",
            sessionEpoch: UUID()
        )
        let manifest = try XCTUnwrap(files(named: "job.json", under: root).first)
        var object = try XCTUnwrap(
            JSONSerialization.jsonObject(with: Data(contentsOf: manifest)) as? [String: Any]
        )
        XCTAssertEqual(object["formatVersion"] as? Int, 3)
        object.removeValue(forKey: "workload")
        object.removeValue(forKey: "activation")
        object["formatVersion"] = 1
        try JSONSerialization.data(withJSONObject: object).write(to: manifest, options: .atomic)

        let recovered = try await MediaStudioVideoJobStore(root: root).recover(
            owner: "alice",
            sessionEpoch: UUID()
        )
        XCTAssertEqual(recovered?.snapshot.id, prepared.snapshot.id)
        XCTAssertEqual(recovered?.snapshot.workload, .genericStudio)
        XCTAssertEqual(recovered?.mode, .create)
    }

    func testVersionTwoManifestWithoutActivationRemainsActive() async throws {
        let root = try directory().appendingPathComponent("jobs")
        let store = MediaStudioVideoJobStore(root: root)
        let prepared = try await store.prepare(
            request: request(prompt: "version two generic"),
            owner: "alice",
            sessionEpoch: UUID()
        )
        let manifest = try XCTUnwrap(files(named: "job.json", under: root).first)
        var object = try XCTUnwrap(
            JSONSerialization.jsonObject(with: Data(contentsOf: manifest)) as? [String: Any]
        )
        XCTAssertEqual(object["formatVersion"] as? Int, 3)
        object.removeValue(forKey: "activation")
        object["formatVersion"] = 2
        try JSONSerialization.data(withJSONObject: object).write(to: manifest, options: .atomic)

        let recovered = try await MediaStudioVideoJobStore(root: root).recover(
            owner: "alice",
            sessionEpoch: UUID()
        )
        XCTAssertEqual(recovered?.snapshot.id, prepared.snapshot.id)
        XCTAssertEqual(recovered?.snapshot.workload, .genericStudio)
        XCTAssertEqual(recovered?.snapshot.isActive, true)
        XCTAssertEqual(recovered?.mode, .create)
    }

    func testSymlinkWriterLockIsRejected() async throws {
        let root = try directory().appendingPathComponent("jobs")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let target = root.appendingPathComponent("target")
        try Data().write(to: target)
        try FileManager.default.createSymbolicLink(
            at: root.appendingPathComponent(".writer.lock"),
            withDestinationURL: target
        )

        do {
            _ = try await MediaStudioVideoJobStore(root: root).recover(
                owner: "alice",
                sessionEpoch: UUID()
            )
            XCTFail("A symlink must never be accepted as the shared writer lock")
        } catch {
            XCTAssertEqual(
                error.localizedDescription,
                MediaStudioVideoJobStoreError.invalidWriterLock.localizedDescription
            )
        }
    }

    func testIndependentStoresSerializeSubmittingTransition() async throws {
        let root = try directory().appendingPathComponent("jobs")
        let initial = MediaStudioVideoJobStore(root: root)
        let prepared = try await initial.prepare(
            request: request(prompt: "submit once"),
            owner: "alice",
            sessionEpoch: UUID()
        )
        let first = MediaStudioVideoJobStore(root: root)
        let second = MediaStudioVideoJobStore(root: root)
        let firstEpoch = prepared.snapshot.sessionEpoch
        let secondEpoch = prepared.snapshot.sessionEpoch

        let firstAttempt = Task { () -> String in
            do {
                _ = try await first.markSubmitting(
                    id: prepared.snapshot.id,
                    owner: "alice",
                    sessionEpoch: firstEpoch
                )
                return "success"
            } catch {
                return error.localizedDescription
            }
        }
        let secondAttempt = Task { () -> String in
            do {
                _ = try await second.markSubmitting(
                    id: prepared.snapshot.id,
                    owner: "alice",
                    sessionEpoch: secondEpoch
                )
                return "success"
            } catch {
                return error.localizedDescription
            }
        }

        let outcomes = await [firstAttempt.value, secondAttempt.value]
        XCTAssertEqual(outcomes.filter { $0 == "success" }.count, 1)
        XCTAssertEqual(
            outcomes.filter {
                $0 == MediaStudioVideoJobStoreError.invalidTransition.localizedDescription
            }.count,
            1
        )
    }

    func testConcurrentRecoveryAllowsOneExecutionOwnerAndDoesNotStealItsEpoch() async throws {
        let root = try directory().appendingPathComponent("jobs")
        let initial = MediaStudioVideoJobStore(root: root)
        let prepared = try await initial.prepare(
            request: request(prompt: "single recovery owner"),
            owner: "alice",
            sessionEpoch: UUID()
        )
        _ = try await initial.markSubmitting(
            id: prepared.snapshot.id,
            owner: "alice",
            sessionEpoch: prepared.snapshot.sessionEpoch
        )
        try await initial.recordProgress(
            id: prepared.snapshot.id,
            owner: "alice",
            progress: .init(status: "queued", jobID: "provider-concurrent")
        )
        let first = MediaStudioVideoJobStore(root: root)
        let second = MediaStudioVideoJobStore(root: root)
        let firstEpoch = UUID()
        let secondEpoch = UUID()

        let firstTask = Task {
            try await first.recover(owner: "alice", sessionEpoch: firstEpoch)
        }
        let secondTask = Task {
            try await second.recover(owner: "alice", sessionEpoch: secondEpoch)
        }
        var recoveries: [(epoch: UUID, execution: MediaStudioVideoJobStore.Execution)] = []
        var errors: [any Error] = []
        do {
            if let value = try await firstTask.value {
                recoveries.append((firstEpoch, value))
            }
        } catch {
            errors.append(error)
        }
        do {
            if let value = try await secondTask.value {
                recoveries.append((secondEpoch, value))
            }
        } catch {
            errors.append(error)
        }
        XCTAssertEqual(recoveries.count, 1)
        XCTAssertEqual(errors.count, 1)
        XCTAssertEqual(
            errors.first?.localizedDescription,
            MediaStudioVideoJobStoreError.jobBusy.localizedDescription
        )
        XCTAssertEqual(recoveries.first?.execution.mode, .resume(providerJobID: "provider-concurrent"))

        let snapshot = try await initial.snapshot(id: prepared.snapshot.id, owner: "alice")
        let winningEpoch = try XCTUnwrap(recoveries.first?.epoch)
        XCTAssertEqual(snapshot.sessionEpoch, winningEpoch)
        let staleEpoch = winningEpoch == firstEpoch ? secondEpoch : firstEpoch
        do {
            try await initial.recordProgress(
                id: prepared.snapshot.id,
                owner: "alice",
                sessionEpoch: staleEpoch,
                progress: .init(status: "running", jobID: "provider-concurrent")
            )
            XCTFail("The recovery epoch that lost ownership must be fenced")
        } catch {
            XCTAssertEqual(
                error.localizedDescription,
                MediaStudioVideoJobStoreError.staleSession.localizedDescription
            )
        }
        withExtendedLifetime(recoveries) {}
    }

    func testExecutionLeaseSurvivesSubmittingTransitionAndReleasesWithExecution() async throws {
        let root = try directory().appendingPathComponent("jobs")
        let epoch = UUID()
        let owner = MediaStudioVideoJobStore(root: root)
        var execution: MediaStudioVideoJobStore.Execution? = try await owner.prepare(
            request: request(prompt: "exclusive execution"),
            owner: "alice",
            sessionEpoch: epoch
        )
        let id = try XCTUnwrap(execution).snapshot.id
        let contender = MediaStudioVideoJobStore(root: root)

        execution = try await owner.markSubmitting(
            try XCTUnwrap(execution),
            owner: "alice",
            sessionEpoch: epoch
        )
        let executionLock = ownerDirectory(root: root, owner: "alice")
            .appendingPathComponent(id.uuidString.lowercased(), isDirectory: true)
            .appendingPathComponent(".execution.lock")
        let lockValues = try executionLock.resourceValues(forKeys: [.isRegularFileKey])
        let lockAttributes = try FileManager.default.attributesOfItem(
            atPath: executionLock.path
        )
        XCTAssertEqual(lockValues.isRegularFile, true)
        XCTAssertEqual(
            (lockAttributes[.posixPermissions] as? NSNumber)?.intValue,
            0o600
        )
        do {
            _ = try await contender.recover(owner: "alice", sessionEpoch: UUID())
            XCTFail("markSubmitting must transfer, not drop, the execution lease")
        } catch {
            XCTAssertEqual(
                error.localizedDescription,
                MediaStudioVideoJobStoreError.jobBusy.localizedDescription
            )
        }
        let snapshotWhileBusy = try await owner.snapshot(id: id, owner: "alice")
        XCTAssertEqual(snapshotWhileBusy.sessionEpoch, epoch)

        execution = nil
        let reviewed = try await contender.recover(owner: "alice", sessionEpoch: UUID())
        XCTAssertEqual(reviewed?.mode, .needsReview)
        XCTAssertEqual(reviewed?.snapshot.status, .submissionUncertain)
    }

    func testSymlinkExecutionLeaseIsRejectedBeforeSubmitting() async throws {
        let directory = try directory()
        let root = directory.appendingPathComponent("jobs")
        let epoch = UUID()
        let store = MediaStudioVideoJobStore(root: root)
        let prepared = try await store.prepare(
            request: request(prompt: "reject execution symlink"),
            owner: "alice",
            sessionEpoch: epoch
        )
        let jobFolder = ownerDirectory(root: root, owner: "alice").appendingPathComponent(
            prepared.snapshot.id.uuidString.lowercased(),
            isDirectory: true
        )
        let target = directory.appendingPathComponent("foreign-lock")
        try Data().write(to: target)
        try FileManager.default.createSymbolicLink(
            at: jobFolder.appendingPathComponent(".execution.lock"),
            withDestinationURL: target
        )

        do {
            _ = try await store.markSubmitting(
                prepared,
                owner: "alice",
                sessionEpoch: epoch
            )
            XCTFail("A symlink must not become the execution lease inode")
        } catch {
            XCTAssertEqual(
                error.localizedDescription,
                MediaStudioVideoJobStoreError.invalidWriterLock.localizedDescription
            )
        }
        let snapshot = try await store.snapshot(id: prepared.snapshot.id, owner: "alice")
        XCTAssertEqual(snapshot.status, .prepared)
    }

    func testPreparedRecoveryPersistsTakeoverEpochForSubmittingWriter() async throws {
        let root = try directory().appendingPathComponent("jobs")
        let initial = MediaStudioVideoJobStore(root: root)
        var prepared: MediaStudioVideoJobStore.Execution? = try await initial.prepare(
            request: request(prompt: "prepared takeover"),
            owner: "alice",
            sessionEpoch: UUID()
        )
        let id = try XCTUnwrap(prepared).snapshot.id
        prepared = nil
        let takeoverEpoch = UUID()
        let restarted = MediaStudioVideoJobStore(root: root)
        let recoveredValue = try await restarted.recover(
            owner: "alice",
            sessionEpoch: takeoverEpoch
        )
        let recovered = try XCTUnwrap(recoveredValue)

        let submitting = try await restarted.markSubmitting(
            recovered,
            owner: "alice",
            sessionEpoch: takeoverEpoch
        )

        XCTAssertEqual(submitting.snapshot.id, id)
        XCTAssertEqual(submitting.snapshot.sessionEpoch, takeoverEpoch)
        XCTAssertEqual(submitting.snapshot.status, .submitting)
    }

    func testClosingWriterLockAllowsWaitingStoreToContinue() async throws {
        let root = try directory().appendingPathComponent("jobs")
        let initial = MediaStudioVideoJobStore(root: root)
        let prepared = try await initial.prepare(
            request: request(prompt: "release lock"),
            owner: "alice",
            sessionEpoch: UUID()
        )
        let lockURL = root.appendingPathComponent(".writer.lock")
        let descriptor = lockURL.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDWR | O_NOFOLLOW | O_CLOEXEC)
        }
        XCTAssertGreaterThanOrEqual(descriptor, 0)
        guard descriptor >= 0 else { return }
        var descriptorIsOpen = true
        defer {
            if descriptorIsOpen {
                _ = flock(descriptor, LOCK_UN)
                _ = Darwin.close(descriptor)
            }
        }
        XCTAssertEqual(flock(descriptor, LOCK_EX), 0)
        let attempted = expectation(description: "second store attempts locked operation")
        let waiting = MediaStudioVideoJobStore(root: root)
        let operation = Task {
            attempted.fulfill()
            return try await waiting.snapshot(id: prepared.snapshot.id, owner: "alice")
        }
        await fulfillment(of: [attempted], timeout: 1)
        try await Task.sleep(for: .milliseconds(25))

        XCTAssertEqual(flock(descriptor, LOCK_UN), 0)
        XCTAssertEqual(Darwin.close(descriptor), 0)
        descriptorIsOpen = false
        let snapshot = try await operation.value
        XCTAssertEqual(snapshot.id, prepared.snapshot.id)
    }

    func testPrepareStagesInputBeforeWriterLockBecomesAvailable() async throws {
        let directory = try directory()
        let root = directory.appendingPathComponent("jobs")
        let initial = MediaStudioVideoJobStore(root: root)
        _ = try await initial.prepare(
            request: request(prompt: "seed lock"),
            owner: "alice",
            sessionEpoch: UUID()
        )
        let source = directory.appendingPathComponent("large-input.mov")
        let bytes = Data(repeating: 0x5A, count: 2 * 1_024 * 1_024)
        try bytes.write(to: source)
        let lockURL = root.appendingPathComponent(".writer.lock")
        let descriptor = lockURL.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDWR | O_NOFOLLOW | O_CLOEXEC)
        }
        XCTAssertGreaterThanOrEqual(descriptor, 0)
        guard descriptor >= 0 else { return }
        var descriptorIsOpen = true
        defer {
            if descriptorIsOpen {
                _ = flock(descriptor, LOCK_UN)
                _ = Darwin.close(descriptor)
            }
        }
        XCTAssertEqual(flock(descriptor, LOCK_EX), 0)
        let attempted = expectation(description: "prepare starts while writer lock is held")
        let store = MediaStudioVideoJobStore(root: root)
        let operation = Task {
            attempted.fulfill()
            return try await store.prepare(
                request: .init(
                    modelConfigID: "video-model",
                    prompt: "stage outside lock",
                    size: "768P",
                    seconds: 4,
                    referenceVideo: .init(
                        name: "large-input.mov",
                        mimeType: "video/quicktime",
                        fileURL: source
                    )
                ),
                owner: "alice",
                sessionEpoch: UUID()
            )
        }
        await fulfillment(of: [attempted], timeout: 1)

        for _ in 0..<200 {
            if !files(named: "referenceVideo.artifact", under: root).isEmpty { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertEqual(files(named: "referenceVideo.artifact", under: root).count, 1)
        XCTAssertEqual(files(named: "job.json", under: root).count, 1)

        XCTAssertEqual(flock(descriptor, LOCK_UN), 0)
        XCTAssertEqual(Darwin.close(descriptor), 0)
        descriptorIsOpen = false
        let prepared = try await operation.value
        let durableInput = try XCTUnwrap(prepared.request.referenceVideo?.fileURL)
        XCTAssertEqual(try Data(contentsOf: durableInput), bytes)
        XCTAssertEqual(files(named: "job.json", under: root).count, 2)
    }

    func testPreparedIntentCopiesInputAndKeepsStableSubmissionIdentityAcrossRestart() async throws {
        let root = try directory()
        let source = root.appendingPathComponent("selected.png")
        let bytes = Data(repeating: 0x5A, count: 4_096)
        try bytes.write(to: source)
        let first = MediaStudioVideoJobStore(root: root.appendingPathComponent("jobs"))
        let prepared = try await first.prepare(
            request: .init(
                modelConfigID: "video-model",
                prompt: "durable prompt",
                size: "768P",
                seconds: 4,
                inputImage: .init(name: "selected.png", mimeType: "image/png", fileURL: source)
            ),
            owner: "alice",
            sessionEpoch: UUID()
        )

        XCTAssertEqual(prepared.mode, .create)
        XCTAssertEqual(prepared.request.clientRequestID, prepared.snapshot.idempotencyKey)
        let durableInput = try XCTUnwrap(prepared.request.inputImage?.fileURL)
        XCTAssertNotEqual(durableInput.standardizedFileURL, source.standardizedFileURL)
        XCTAssertEqual(try Data(contentsOf: durableInput), bytes)

        let restarted = MediaStudioVideoJobStore(root: root.appendingPathComponent("jobs"))
        let recoveredValue = try await restarted.recover(
            owner: "alice",
            sessionEpoch: UUID()
        )
        let recovered = try XCTUnwrap(recoveredValue)
        XCTAssertEqual(recovered.mode, .create)
        XCTAssertEqual(recovered.snapshot.id, prepared.snapshot.id)
        XCTAssertEqual(recovered.request.clientRequestID, prepared.snapshot.idempotencyKey)
        XCTAssertEqual(try Data(contentsOf: XCTUnwrap(recovered.request.inputImage?.fileURL)), bytes)
    }

    func testSubmittingWithoutProviderIdentityBecomesUncertainAndCannotBeRecreated() async throws {
        let root = try directory().appendingPathComponent("jobs")
        let first = MediaStudioVideoJobStore(root: root)
        let prepared = try await first.prepare(
            request: .init(
                modelConfigID: "video-model",
                prompt: "possibly accepted",
                size: "768P",
                seconds: 4
            ),
            owner: "alice",
            sessionEpoch: UUID()
        )
        _ = try await first.markSubmitting(
            id: prepared.snapshot.id,
            owner: "alice",
            sessionEpoch: prepared.snapshot.sessionEpoch
        )

        let restarted = MediaStudioVideoJobStore(root: root)
        let recoveredValue = try await restarted.recover(
            owner: "alice",
            sessionEpoch: UUID()
        )
        let recovered = try XCTUnwrap(recoveredValue)
        XCTAssertEqual(recovered.mode, .needsReview)
        XCTAssertEqual(recovered.snapshot.status, .submissionUncertain)
        XCTAssertNil(recovered.snapshot.providerJobID)
        XCTAssertFalse(recovered.snapshot.retryable)

        let recoveredAgainValue = try await restarted.recover(
            owner: "alice",
            sessionEpoch: UUID()
        )
        let recoveredAgain = try XCTUnwrap(recoveredAgainValue)
        XCTAssertEqual(recoveredAgain.mode, .needsReview)
        XCTAssertEqual(recoveredAgain.snapshot.id, prepared.snapshot.id)
    }

    func testChangedDurableInputIsRejectedBeforeAnyRecoveredSubmission() async throws {
        let root = try directory()
        let source = root.appendingPathComponent("selected.png")
        try Data(repeating: 0x11, count: 1_024).write(to: source)
        let jobRoot = root.appendingPathComponent("jobs")
        let store = MediaStudioVideoJobStore(root: jobRoot)
        let prepared = try await store.prepare(
            request: .init(
                modelConfigID: "video-model",
                prompt: "immutable input",
                size: "768P",
                seconds: 4,
                inputImage: .init(name: "selected.png", mimeType: "image/png", fileURL: source)
            ),
            owner: "alice",
            sessionEpoch: UUID()
        )
        let durableURL = try XCTUnwrap(prepared.request.inputImage?.fileURL)
        try Data(repeating: 0x22, count: 1_024).write(to: durableURL)

        do {
            _ = try await MediaStudioVideoJobStore(root: jobRoot).recover(
                owner: "alice",
                sessionEpoch: UUID()
            )
            XCTFail("Changed durable input must not be submitted")
        } catch {
            XCTAssertEqual(
                error.localizedDescription,
                MediaStudioVideoJobStoreError.invalidInput.localizedDescription
            )
        }
    }

    func testCorruptCommittedManifestBlocksRecoveryInsteadOfLookingLikeNoJob() async throws {
        let root = try directory()
        let jobRoot = root.appendingPathComponent("jobs")
        let store = MediaStudioVideoJobStore(root: jobRoot)
        _ = try await store.prepare(
            request: .init(
                modelConfigID: "video-model",
                prompt: "committed intent",
                size: "768P",
                seconds: 4
            ),
            owner: "alice",
            sessionEpoch: UUID()
        )
        let enumerator = try XCTUnwrap(FileManager.default.enumerator(
            at: jobRoot,
            includingPropertiesForKeys: [.isRegularFileKey]
        ))
        let manifestURL = try XCTUnwrap(
            enumerator.compactMap { $0 as? URL }.first { $0.lastPathComponent == "job.json" }
        )
        try Data("broken".utf8).write(to: manifestURL)

        do {
            _ = try await MediaStudioVideoJobStore(root: jobRoot).recover(
                owner: "alice",
                sessionEpoch: UUID()
            )
            XCTFail("A corrupt committed job must block recovery")
        } catch {
            XCTAssertFalse(error.localizedDescription.isEmpty)
        }
    }

    func testProviderIdentityResumesWithoutCreateAndReleasesSubmissionInputs() async throws {
        let root = try directory()
        let source = root.appendingPathComponent("selected.png")
        try Data(repeating: 0x44, count: 2_048).write(to: source)
        let jobRoot = root.appendingPathComponent("jobs")
        let first = MediaStudioVideoJobStore(root: jobRoot)
        let prepared = try await first.prepare(
            request: .init(
                modelConfigID: "video-model",
                prompt: "resume me",
                size: "768P",
                seconds: 4,
                inputImage: .init(name: "selected.png", mimeType: "image/png", fileURL: source)
            ),
            owner: "alice",
            sessionEpoch: UUID()
        )
        _ = try await first.markSubmitting(
            id: prepared.snapshot.id,
            owner: "alice",
            sessionEpoch: prepared.snapshot.sessionEpoch
        )
        try await first.recordProgress(
            id: prepared.snapshot.id,
            owner: "alice",
            progress: .init(status: "queued", jobID: "provider-42")
        )

        let restarted = MediaStudioVideoJobStore(root: jobRoot)
        let recoveredValue = try await restarted.recover(
            owner: "alice",
            sessionEpoch: UUID()
        )
        let recovered = try XCTUnwrap(recoveredValue)
        XCTAssertEqual(recovered.mode, .resume(providerJobID: "provider-42"))
        XCTAssertEqual(recovered.snapshot.retryCount, 1)
        XCTAssertNil(recovered.request.inputImage)
        XCTAssertEqual(recovered.request.clientRequestID, prepared.snapshot.idempotencyKey)
    }

    func testRecoverySessionEpochFencesLateWriterFromPreviousSession() async throws {
        let root = try directory().appendingPathComponent("jobs")
        let oldEpoch = UUID()
        let newEpoch = UUID()
        let store = MediaStudioVideoJobStore(root: root)
        let prepared = try await store.prepare(
            request: .init(
                modelConfigID: "video-model",
                prompt: "fenced",
                size: "768P",
                seconds: 4
            ),
            owner: "alice",
            sessionEpoch: oldEpoch
        )
        _ = try await store.markSubmitting(
            id: prepared.snapshot.id,
            owner: "alice",
            sessionEpoch: oldEpoch
        )
        try await store.recordProgress(
            id: prepared.snapshot.id,
            owner: "alice",
            sessionEpoch: oldEpoch,
            progress: .init(status: "queued", jobID: "provider-fenced")
        )
        _ = try await store.recover(owner: "alice", sessionEpoch: newEpoch)

        do {
            try await store.recordProgress(
                id: prepared.snapshot.id,
                owner: "alice",
                sessionEpoch: oldEpoch,
                progress: .init(status: "failed", jobID: "provider-fenced")
            )
            XCTFail("Previous session must not advance recovered job state")
        } catch {
            XCTAssertEqual(
                error.localizedDescription,
                MediaStudioVideoJobStoreError.staleSession.localizedDescription
            )
        }
        let snapshot = try await store.snapshot(id: prepared.snapshot.id, owner: "alice")
        XCTAssertEqual(snapshot.sessionEpoch, newEpoch)
        XCTAssertNotEqual(snapshot.status, .failed)
    }

    func testServiceNeverSubmitsAnUncertainRecoveredJob() async throws {
        let root = try directory()
        let store = MediaStudioVideoJobStore(root: root.appendingPathComponent("jobs"))
        let prepared = try await store.prepare(
            request: .init(
                modelConfigID: "video-model",
                prompt: "do not duplicate",
                size: "768P",
                seconds: 4
            ),
            owner: "alice",
            sessionEpoch: UUID()
        )
        _ = try await store.markSubmitting(
            id: prepared.snapshot.id,
            owner: "alice",
            sessionEpoch: prepared.snapshot.sessionEpoch
        )
        let generator = DurableVideoTestService()
        let history = DurableVideoTestHistory(root: root.appendingPathComponent("history"))
        let service = MediaStudioVideoJobService(
            generator: generator,
            history: history,
            store: MediaStudioVideoJobStore(root: root.appendingPathComponent("jobs"))
        )

        let result = try await service.recover(
            owner: "alice",
            sessionEpoch: UUID(),
            progress: { _ in }
        )
        guard case let .needsReview(progress, _) = result else {
            return XCTFail("Expected uncertain submission review")
        }
        XCTAssertEqual(progress.status, "submission_uncertain")
        let calls = await generator.calls()
        XCTAssertEqual(calls.creates, 0)
        XCTAssertEqual(calls.resumes, 0)
    }

    func testServiceResumesProviderJobAndCompletesDurableRecord() async throws {
        let root = try directory()
        let jobRoot = root.appendingPathComponent("jobs")
        let store = MediaStudioVideoJobStore(root: jobRoot)
        let prepared = try await store.prepare(
            request: .init(
                modelConfigID: "video-model",
                prompt: "finish after restart",
                size: "768P",
                seconds: 4
            ),
            owner: "alice",
            sessionEpoch: UUID()
        )
        _ = try await store.markSubmitting(
            id: prepared.snapshot.id,
            owner: "alice",
            sessionEpoch: prepared.snapshot.sessionEpoch
        )
        try await store.recordProgress(
            id: prepared.snapshot.id,
            owner: "alice",
            progress: .init(status: "queued", jobID: "provider-77")
        )
        let generator = DurableVideoTestService()
        let service = MediaStudioVideoJobService(
            generator: generator,
            history: DurableVideoTestHistory(root: root.appendingPathComponent("history")),
            store: MediaStudioVideoJobStore(root: jobRoot)
        )

        let result = try await service.recover(
            owner: "alice",
            sessionEpoch: UUID(),
            progress: { _ in }
        )
        guard case let .finished(.saved(item)) = result else {
            return XCTFail("Expected resumed video to finish")
        }
        XCTAssertEqual(item.prompt, "finish after restart")
        let calls = await generator.calls()
        XCTAssertEqual(calls.creates, 0)
        XCTAssertEqual(calls.resumes, 1)
        XCTAssertEqual(calls.lastProviderJobID, "provider-77")
        let snapshot = try await store.snapshot(id: prepared.snapshot.id, owner: "alice")
        XCTAssertEqual(snapshot.status, .completed)
        XCTAssertEqual(snapshot.providerJobID, "provider-77")
    }

    func testFinalizeRetryReusesCommittedHistoryRecordInsteadOfDuplicatingVideo() async throws {
        let root = try directory()
        let jobRoot = root.appendingPathComponent("jobs")
        let history = MediaStudioHistoryStore(root: root.appendingPathComponent("history"))
        let store = MediaStudioVideoJobStore(root: jobRoot)
        let prepared = try await store.prepare(
            request: .init(
                modelConfigID: "video-model",
                prompt: "commit exactly once",
                size: "768P",
                seconds: 4
            ),
            owner: "alice",
            sessionEpoch: UUID()
        )
        _ = try await store.markSubmitting(
            id: prepared.snapshot.id,
            owner: "alice",
            sessionEpoch: prepared.snapshot.sessionEpoch
        )
        try await store.recordProgress(
            id: prepared.snapshot.id,
            owner: "alice",
            progress: .init(status: "completed", jobID: "provider-finalize")
        )
        let providerResult = VideoGenerationResult(
            id: "provider-finalize",
            modelConfigID: "video-model",
            modelName: "MiniMax-H3",
            createdAt: "2026-10-22T00:00:00Z",
            mimeType: "video/mp4",
            videoData: Data("video".utf8)
        )
        _ = try await history.saveVideo(
            providerResult,
            prompt: "commit exactly once",
            owner: "alice",
            durableRecordID: prepared.snapshot.id.uuidString.lowercased()
        )

        let service = MediaStudioVideoJobService(
            generator: DurableVideoTestService(),
            history: history,
            store: MediaStudioVideoJobStore(root: jobRoot)
        )
        _ = try await service.recover(
            owner: "alice",
            sessionEpoch: UUID(),
            progress: { _ in }
        )

        let page = try await history.load(owner: "alice")
        XCTAssertEqual(page.videos.count, 1)
        XCTAssertEqual(page.videos.first?.id, prepared.snapshot.id.uuidString.lowercased())
        XCTAssertEqual(page.videos.first?.prompt, "commit exactly once")
    }

    @MainActor
    func testViewModelActivationRestoresProviderJobWithoutUserResubmission() async throws {
        let root = try directory()
        let jobRoot = root.appendingPathComponent("jobs")
        let store = MediaStudioVideoJobStore(root: jobRoot)
        let prepared = try await store.prepare(
            request: .init(
                modelConfigID: "video-model",
                prompt: "restore in studio",
                size: "768P",
                seconds: 4
            ),
            owner: "alice",
            sessionEpoch: UUID()
        )
        _ = try await store.markSubmitting(
            id: prepared.snapshot.id,
            owner: "alice",
            sessionEpoch: prepared.snapshot.sessionEpoch
        )
        try await store.recordProgress(
            id: prepared.snapshot.id,
            owner: "alice",
            progress: .init(status: "queued", jobID: "provider-vm")
        )
        let generator = DurableVideoTestService()
        let viewModel = MediaStudioViewModel(
            service: generator,
            historyStore: MediaStudioHistoryStore(root: root.appendingPathComponent("history")),
            videoJobStore: MediaStudioVideoJobStore(root: jobRoot)
        )

        viewModel.activate(userID: "alice")
        for _ in 0..<200 {
            if !viewModel.isRecoveringVideoJob && !viewModel.isGeneratingVideo,
               viewModel.videoHistory.first?.prompt == "restore in studio" {
                break
            }
            try await Task.sleep(for: .milliseconds(10))
        }

        XCTAssertEqual(viewModel.videoHistory.first?.prompt, "restore in studio")
        XCTAssertEqual(viewModel.videoProgress?.status, "completed")
        let calls = await generator.calls()
        XCTAssertEqual(calls.creates, 0)
        XCTAssertEqual(calls.resumes, 1)
        XCTAssertEqual(calls.lastProviderJobID, "provider-vm")
    }
}

private actor DurableVideoTestService: ResumableVideoGenerationServicing {
    private var createCount = 0
    private var resumeCount = 0
    private var providerJobID: String?

    func fetchModels() async throws -> [MediaGenerationModel] { [] }

    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult {
        throw URLError(.unsupportedURL)
    }

    func generateVideo(
        _ request: VideoGenerationRequest,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> VideoGenerationResult {
        createCount += 1
        return Self.result(id: "unexpected-create")
    }

    func resumeVideo(
        _ request: VideoGenerationRequest,
        jobID: String,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> VideoGenerationResult {
        resumeCount += 1
        providerJobID = jobID
        await progress(.init(status: "completed", percent: 100, jobID: jobID))
        await progress(.init(status: "downloading", jobID: jobID))
        return Self.result(id: jobID)
    }

    func calls() -> (creates: Int, resumes: Int, lastProviderJobID: String?) {
        (createCount, resumeCount, providerJobID)
    }

    nonisolated private static func result(id: String) -> VideoGenerationResult {
        .init(
            id: id,
            modelConfigID: "video-model",
            modelName: "MiniMax-H3",
            createdAt: "2026-10-22T00:00:00Z",
            mimeType: "video/mp4",
            videoData: Data("video".utf8)
        )
    }
}

private actor DurableVideoTestHistory: MediaStudioDurableVideoHistoryPersisting {
    let root: URL

    init(root: URL) { self.root = root }

    func saveVideo(
        _ result: VideoGenerationResult,
        prompt: String,
        owner: String
    ) async throws -> MediaStudioVideoHistoryItem {
        try await saveVideo(
            result,
            prompt: prompt,
            owner: owner,
            durableRecordID: result.id
        )
    }

    func saveVideo(
        _ result: VideoGenerationResult,
        prompt: String,
        owner: String,
        durableRecordID: String
    ) async throws -> MediaStudioVideoHistoryItem {
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let url = root.appendingPathComponent("\(durableRecordID).mp4")
        try result.videoData.write(to: url)
        return .init(
            id: result.id,
            prompt: prompt,
            modelName: result.modelName,
            createdAt: Date(timeIntervalSince1970: 1),
            fileURL: url
        )
    }
}
