import ChatOSCore
import CryptoKit
import Darwin
import Foundation

/// Durable, account-scoped source of truth for Media Studio video jobs. Workload partitions
/// reserve stable Story/Batch associations, while those callers remain on their legacy stores
/// until the architecture plan's atomic migration is implemented.
actor MediaStudioVideoJobStore {
    struct Workload: Codable, Sendable, Equatable {
        enum Kind: String, Codable, Sendable {
            case genericStudio = "generic_studio"
            case storyVideo = "story_video"
            case storyBatchVideo = "story_batch_video"
        }

        var kind: Kind
        var projectID: UUID?
        var segmentID: String?
        var attemptID: UUID?
        var batchID: UUID?

        static let genericStudio = Workload(kind: .genericStudio)

        static func storyVideo(
            projectID: UUID,
            segmentID: String,
            attemptID: UUID
        ) -> Workload {
            .init(
                kind: .storyVideo,
                projectID: projectID,
                segmentID: segmentID,
                attemptID: attemptID
            )
        }

        static func storyBatchVideo(
            projectID: UUID,
            batchID: UUID,
            segmentID: String,
            attemptID: UUID
        ) -> Workload {
            .init(
                kind: .storyBatchVideo,
                projectID: projectID,
                segmentID: segmentID,
                attemptID: attemptID,
                batchID: batchID
            )
        }

        private init(
            kind: Kind,
            projectID: UUID? = nil,
            segmentID: String? = nil,
            attemptID: UUID? = nil,
            batchID: UUID? = nil
        ) {
            self.kind = kind
            self.projectID = projectID
            self.segmentID = segmentID
            self.attemptID = attemptID
            self.batchID = batchID
        }

        fileprivate var isValid: Bool {
            let validSegment = segmentID.map {
                !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && $0.count <= 128
            } ?? false
            return switch kind {
            case .genericStudio:
                projectID == nil && segmentID == nil && attemptID == nil && batchID == nil
            case .storyVideo:
                projectID != nil && validSegment && attemptID != nil && batchID == nil
            case .storyBatchVideo:
                projectID != nil && validSegment && attemptID != nil && batchID != nil
            }
        }
    }

    enum Status: String, Codable, Sendable {
        case prepared
        case submitting
        case submitted
        case running
        case downloading
        case validating
        case finalizing
        case completed
        case submissionUncertain = "submission_uncertain"
        case cancellationRequested = "cancellation_requested"
        case cancelled
        case failed

        var isTerminal: Bool {
            self == .completed || self == .cancelled
        }
    }

    enum RecoveryMode: Sendable, Equatable {
        case create
        case resume(providerJobID: String)
        case needsReview
    }

    private enum Activation: String, Codable, Sendable {
        case staged
        case active
    }

    struct Snapshot: Sendable, Equatable {
        var id: UUID
        var ownerUserID: String
        var workload: Workload
        var isActive: Bool
        var sessionEpoch: UUID
        var idempotencyKey: String
        var providerJobID: String?
        var status: Status
        var retryCount: Int
        var stateVersion: Int
        var failurePhase: String?
        var failureMessage: String?
        var retryable: Bool
        var updatedAt: Date
    }

    struct Execution: Sendable {
        var snapshot: Snapshot
        var request: VideoGenerationRequest
        var mode: RecoveryMode
        fileprivate var lease: ExecutionLease?
    }

    fileprivate final class ExecutionLease: @unchecked Sendable {
        private var descriptor: Int32?

        init(descriptor: Int32) {
            self.descriptor = descriptor
        }

        deinit {
            guard let descriptor else { return }
            _ = flock(descriptor, LOCK_UN)
            _ = Darwin.close(descriptor)
        }
    }

    private enum InputKind: String, Codable, Hashable {
        case firstFrame
        case lastFrame
        case referenceVideo
        case referenceAudio

        var maximumBytes: Int {
            switch self {
            case .firstFrame, .lastFrame:
                MediaStudioInputArtifactStore.maximumBytes
            case .referenceVideo:
                47 * 1_024 * 1_024
            case .referenceAudio:
                MediaStudioInputArtifactStore.maximumBytes
            }
        }
    }

    private struct InputArtifact: Codable, Equatable {
        var kind: InputKind
        var filename: String
        var originalName: String
        var mimeType: String
        var byteCount: Int
        var sha256: String
    }

    private struct StoredRequest: Codable, Equatable {
        var modelConfigID: String
        var prompt: String
        var size: String
        var seconds: Int
        var referencePurpose: String
        var ratio: String
        var inputs: [InputArtifact]
    }

    private struct Record: Codable {
        var formatVersion: Int
        var id: UUID
        var ownerUserID: String
        /// Missing in format-v1 manifests written before workload partitioning. Those records
        /// are generic Studio jobs by construction and remain readable without rewriting.
        var workload: Workload?
        /// Missing in v1/v2. Those records were published as runnable jobs before the staged
        /// Story ownership-transfer protocol existed and therefore resolve to `active`.
        var activation: Activation?
        var sessionEpoch: UUID
        var idempotencyKey: String
        var providerJobID: String?
        var status: Status
        var retryCount: Int
        var stateVersion: Int
        var failurePhase: String?
        var failureMessage: String?
        var retryable: Bool
        var createdAt: Date
        var updatedAt: Date
        var request: StoredRequest
    }

    private struct StagedRecord {
        var record: Record
        var folder: URL
        var leaseDescriptor: Int32
    }

    private static let formatVersion = 3
    private static let maximumManifestBytes = 1 * 1_024 * 1_024
    private static let stagingLeaseFilename = ".staging.lease"
    private static let executionLeaseFilename = ".execution.lock"
    private static let stagingRetention: TimeInterval = 24 * 60 * 60
    private static let maximumStagingCleanupCandidates = 64
    private let root: URL
    private var maintainedStagingOwners: Set<String> = []

    init(root: URL? = nil) {
        self.root = (root ?? FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        )[0].appendingPathComponent("ChatOSSwift/MediaJobs", isDirectory: true))
            .resolvingSymlinksInPath()
    }

    func prepare(
        request: VideoGenerationRequest,
        owner ownerInput: String,
        sessionEpoch: UUID,
        workload: Workload = .genericStudio
    ) throws -> Execution {
        let owner = ownerInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !owner.isEmpty else { throw MediaStudioVideoJobStoreError.invalidOwner }
        guard workload.isValid else { throw MediaStudioVideoJobStoreError.invalidWorkload }
        guard workload == .genericStudio else {
            throw MediaStudioVideoJobStoreError.activationRequired
        }
        let staged = try makeStagedRecord(
            request: request,
            id: UUID(),
            owner: owner,
            sessionEpoch: sessionEpoch,
            workload: workload,
            activation: .active,
            providerJobID: nil,
            status: .prepared,
            persistsInputs: true
        )
        defer { discard(staged) }
        maintainStaging(owner: owner)
        let record = try publish(staged.record, from: staged.folder)
        return try execution(record, mode: .create, validatesInputs: false)
    }

    /// Stages a new Story job under its stable attempt ID. A staged record is durable but
    /// invisible to recovery until the Story manifest has atomically switched to this ID and
    /// `activate` completes. Repeating the same stage is idempotent; conflicting content is not.
    func stagePrepared(
        request: VideoGenerationRequest,
        id: UUID,
        owner ownerInput: String,
        sessionEpoch: UUID,
        workload: Workload
    ) throws -> Snapshot {
        let owner = ownerInput.trimmingCharacters(in: .whitespacesAndNewlines)
        try validateStagedIdentity(id: id, owner: owner, workload: workload)
        let staged = try makeStagedRecord(
            request: request,
            id: id,
            owner: owner,
            sessionEpoch: sessionEpoch,
            workload: workload,
            activation: .staged,
            providerJobID: nil,
            status: .prepared,
            persistsInputs: true
        )
        defer { discard(staged) }
        maintainStaging(owner: owner)
        return snapshot(try publish(staged.record, from: staged.folder))
    }

    /// Stages an existing provider task for a one-way legacy import. Imported tasks never
    /// contain create inputs and therefore can only resume the supplied provider identity.
    func stageImportedProviderJob(
        request: VideoGenerationRequest,
        id: UUID,
        providerJobID providerJobIDInput: String,
        owner ownerInput: String,
        sessionEpoch: UUID,
        workload: Workload
    ) throws -> Snapshot {
        let owner = ownerInput.trimmingCharacters(in: .whitespacesAndNewlines)
        try validateStagedIdentity(id: id, owner: owner, workload: workload)
        let providerJobID = providerJobIDInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !providerJobID.isEmpty else { throw MediaStudioVideoJobStoreError.invalidProviderJob }
        let staged = try makeStagedRecord(
            request: request,
            id: id,
            owner: owner,
            sessionEpoch: sessionEpoch,
            workload: workload,
            activation: .staged,
            providerJobID: providerJobID,
            status: .submitted,
            persistsInputs: false
        )
        defer { discard(staged) }
        maintainStaging(owner: owner)
        return snapshot(try publish(staged.record, from: staged.folder))
    }

    /// Imports a legacy intent that may already have crossed the provider submission boundary
    /// without returning an identity. It contains no create inputs and can only enter manual
    /// review after activation; recovery must never turn it into a new provider request.
    func stageSubmissionUncertain(
        request: VideoGenerationRequest,
        id: UUID,
        owner ownerInput: String,
        sessionEpoch: UUID,
        workload: Workload
    ) throws -> Snapshot {
        let owner = ownerInput.trimmingCharacters(in: .whitespacesAndNewlines)
        try validateStagedIdentity(id: id, owner: owner, workload: workload)
        let staged = try makeStagedRecord(
            request: request,
            id: id,
            owner: owner,
            sessionEpoch: sessionEpoch,
            workload: workload,
            activation: .staged,
            providerJobID: nil,
            status: .submissionUncertain,
            persistsInputs: false
        )
        defer { discard(staged) }
        maintainStaging(owner: owner)
        return snapshot(try publish(staged.record, from: staged.folder))
    }

    /// Completes the second half of the ownership handoff after the Story manifest points at
    /// this local job ID. Activation is idempotent and is the only operation that makes a
    /// staged Story job visible to recovery or state transitions.
    func activate(
        id: UUID,
        owner ownerInput: String,
        sessionEpoch: UUID,
        workload: Workload
    ) throws -> Execution {
        let owner = ownerInput.trimmingCharacters(in: .whitespacesAndNewlines)
        try validateStagedIdentity(id: id, owner: owner, workload: workload)
        let lockDescriptor = try acquireStoreLock()
        defer { releaseStoreLock(lockDescriptor) }
        var record = try load(id: id, owner: owner)
        guard resolvedWorkload(record) == workload else {
            throw MediaStudioVideoJobStoreError.jobConflict
        }
        let mode = recoveryMode(record)
        let executionLease = try acquireExecutionLeaseIfNeeded(
            id: record.id,
            owner: owner,
            mode: mode
        )
        if resolvedActivation(record) == .staged {
            record.sessionEpoch = sessionEpoch
            record.activation = .active
            transition(&record, to: record.status)
            try save(record)
        } else if record.sessionEpoch != sessionEpoch {
            record.sessionEpoch = sessionEpoch
            record.stateVersion += 1
            record.updatedAt = Date()
            try save(record)
        }
        return try execution(record, mode: mode, lease: executionLease)
    }

    func markSubmitting(id: UUID, owner: String, sessionEpoch: UUID) throws -> Execution {
        try markSubmitting(
            id: id,
            owner: owner,
            sessionEpoch: sessionEpoch,
            executionLease: nil
        )
    }

    func markSubmitting(
        _ execution: Execution,
        owner: String,
        sessionEpoch: UUID
    ) throws -> Execution {
        guard execution.snapshot.ownerUserID == owner,
              execution.snapshot.sessionEpoch == sessionEpoch,
              execution.mode == .create else {
            throw MediaStudioVideoJobStoreError.invalidTransition
        }
        return try markSubmitting(
            id: execution.snapshot.id,
            owner: owner,
            sessionEpoch: sessionEpoch,
            executionLease: execution.lease
        )
    }

    private func markSubmitting(
        id: UUID,
        owner: String,
        sessionEpoch: UUID,
        executionLease: ExecutionLease?
    ) throws -> Execution {
        let lockDescriptor = try acquireStoreLock()
        defer { releaseStoreLock(lockDescriptor) }
        var record = try loadActive(id: id, owner: owner)
        try verify(sessionEpoch: sessionEpoch, record: record)
        guard record.status == .prepared else {
            throw MediaStudioVideoJobStoreError.invalidTransition
        }
        let lease = try executionLease
            ?? acquireExecutionLease(id: record.id, owner: record.ownerUserID)
        record.sessionEpoch = sessionEpoch
        transition(&record, to: .submitting)
        try save(record)
        return try execution(record, mode: .create, lease: lease)
    }

    func recordProgress(
        id: UUID,
        owner: String,
        sessionEpoch: UUID? = nil,
        progress: VideoGenerationProgress
    ) throws {
        let lockDescriptor = try acquireStoreLock()
        defer { releaseStoreLock(lockDescriptor) }
        var record = try loadActive(id: id, owner: owner)
        try verify(sessionEpoch: sessionEpoch, record: record)
        guard !record.status.isTerminal else { return }
        let hadProviderJobID = record.providerJobID != nil
        if let providerJobID = progress.jobID?.trimmingCharacters(in: .whitespacesAndNewlines),
           !providerJobID.isEmpty {
            guard record.providerJobID == nil || record.providerJobID == providerJobID else {
                throw MediaStudioVideoJobStoreError.jobConflict
            }
            record.providerJobID = providerJobID
        }
        let nextStatus: Status
        switch progress.status.lowercased() {
        case "completed": nextStatus = .validating
        case "downloading": nextStatus = .downloading
        case "failed": nextStatus = .failed
        case "queued", "not_start", "submitted": nextStatus = .submitted
        default: nextStatus = record.providerJobID == nil ? .submitting : .running
        }
        transition(&record, to: nextStatus)
        // The provider identity is the critical crash-recovery fact. Commit it before
        // reclaiming upload artifacts so a later cleanup/write failure cannot lose both.
        if !hadProviderJobID && record.providerJobID != nil {
            try save(record)
        }
        if record.providerJobID != nil {
            try removeInputs(for: &record)
        }
        try save(record)
    }

    func markFinalizing(
        id: UUID,
        owner: String,
        sessionEpoch: UUID? = nil,
        providerJobID: String
    ) throws {
        let providerJobID = providerJobID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !providerJobID.isEmpty else {
            throw MediaStudioVideoJobStoreError.invalidProviderJob
        }
        let lockDescriptor = try acquireStoreLock()
        defer { releaseStoreLock(lockDescriptor) }
        var record = try loadActive(id: id, owner: owner)
        try verify(sessionEpoch: sessionEpoch, record: record)
        guard record.providerJobID == nil || record.providerJobID == providerJobID else {
            throw MediaStudioVideoJobStoreError.jobConflict
        }
        if record.status == .completed { return }
        guard record.status != .cancelled else {
            throw MediaStudioVideoJobStoreError.invalidTransition
        }
        record.providerJobID = providerJobID
        transition(&record, to: .finalizing)
        try removeInputs(for: &record)
        try save(record)
    }

    func markCompleted(
        id: UUID,
        owner: String,
        sessionEpoch: UUID? = nil,
        providerJobID: String
    ) throws {
        let providerJobID = providerJobID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !providerJobID.isEmpty else {
            throw MediaStudioVideoJobStoreError.invalidProviderJob
        }
        let lockDescriptor = try acquireStoreLock()
        defer { releaseStoreLock(lockDescriptor) }
        var record = try loadActive(id: id, owner: owner)
        try verify(sessionEpoch: sessionEpoch, record: record)
        guard record.providerJobID == nil || record.providerJobID == providerJobID else {
            throw MediaStudioVideoJobStoreError.jobConflict
        }
        if record.status == .completed { return }
        guard record.status != .cancelled else {
            throw MediaStudioVideoJobStoreError.invalidTransition
        }
        record.providerJobID = providerJobID
        record.failurePhase = nil
        record.failureMessage = nil
        record.retryable = false
        transition(&record, to: .completed)
        try removeInputs(for: &record)
        try save(record)
    }

    func markFailure(
        id: UUID,
        owner: String,
        sessionEpoch: UUID? = nil,
        phase: String,
        message: String,
        requestMayHaveBeenSubmitted: Bool,
        retryable: Bool
    ) throws {
        let lockDescriptor = try acquireStoreLock()
        defer { releaseStoreLock(lockDescriptor) }
        var record = try loadActive(id: id, owner: owner)
        try verify(sessionEpoch: sessionEpoch, record: record)
        guard !record.status.isTerminal else { return }
        record.failurePhase = phase
        record.failureMessage = message
        let uncertain = requestMayHaveBeenSubmitted && record.providerJobID == nil
            && record.status != .prepared
        record.retryable = uncertain ? false : retryable
        transition(&record, to: uncertain ? .submissionUncertain : .failed)
        if !uncertain && record.providerJobID == nil {
            try removeInputs(for: &record)
        }
        try save(record)
    }

    func markCancellation(id: UUID, owner: String, sessionEpoch: UUID? = nil) throws {
        let lockDescriptor = try acquireStoreLock()
        defer { releaseStoreLock(lockDescriptor) }
        var record = try loadActive(id: id, owner: owner)
        try verify(sessionEpoch: sessionEpoch, record: record)
        guard !record.status.isTerminal else { return }
        if record.providerJobID != nil {
            transition(&record, to: .cancellationRequested)
            try removeInputs(for: &record)
        } else if record.status == .prepared {
            transition(&record, to: .cancelled)
            try removeInputs(for: &record)
        } else {
            record.failurePhase = "submitting"
            record.failureMessage = "提交已中断，服务商是否已接单尚未确认。"
            record.retryable = false
            transition(&record, to: .submissionUncertain)
        }
        try save(record)
    }

    /// Records the user's explicit decision to abandon a reviewed task before callers remove
    /// their business association. The manifest and provider identity remain as audit evidence.
    /// Repeating this after a crash is read-only and safe.
    func releaseAfterVerifiedRetry(
        id: UUID,
        owner ownerInput: String,
        sessionEpoch: UUID,
        workload: Workload
    ) throws -> Snapshot {
        let owner = ownerInput.trimmingCharacters(in: .whitespacesAndNewlines)
        try validateStagedIdentity(id: id, owner: owner, workload: workload)
        let lockDescriptor = try acquireStoreLock()
        defer { releaseStoreLock(lockDescriptor) }
        var record = try loadActive(id: id, owner: owner)
        try verify(sessionEpoch: sessionEpoch, record: record)
        guard resolvedWorkload(record) == workload else {
            throw MediaStudioVideoJobStoreError.jobConflict
        }
        if record.status == .cancelled,
           record.failurePhase == "verified_retry_release" {
            return snapshot(record)
        }
        record.failurePhase = "verified_retry_release"
        record.failureMessage = "用户已核对原任务，并明确释放该关联以允许重新生成。"
        record.retryable = false
        transition(&record, to: .cancelled)
        try removeInputs(for: &record)
        try save(record)
        return snapshot(record)
    }

    /// Returns the newest generic Studio job that still requires action. A persisted
    /// `submitting` record without provider identity is reconciled to an explicit uncertain
    /// state and is never converted back to a create request.
    func recover(owner: String, sessionEpoch: UUID) throws -> Execution? {
        try recover(owner: owner, sessionEpoch: sessionEpoch, workload: .genericStudio)
    }

    /// Workload-specific recovery is the only entry point future Story/Batch adapters may use.
    /// It cannot consume or advance another product surface's durable task.
    func recover(
        owner: String,
        sessionEpoch: UUID,
        workload: Workload
    ) throws -> Execution? {
        let normalizedOwner = owner.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalizedOwner.isEmpty else { throw MediaStudioVideoJobStoreError.invalidOwner }
        guard workload.isValid else { throw MediaStudioVideoJobStoreError.invalidWorkload }
        maintainStaging(owner: normalizedOwner)
        let lockDescriptor = try acquireStoreLock()
        defer { releaseStoreLock(lockDescriptor) }
        let records = try loadRecords(owner: normalizedOwner)
            .filter { resolvedWorkload($0) == workload }
            .filter { resolvedActivation($0) == .active }
            .filter(Self.requiresRecovery)
            .sorted { $0.updatedAt > $1.updatedAt }
        guard var record = records.first else { return nil }
        let executionLease = record.status == .submissionUncertain
            ? nil
            : try acquireExecutionLease(id: record.id, owner: normalizedOwner)

        if record.providerJobID == nil && Self.mayHaveCrossedSubmissionBoundary(record.status) {
            record.sessionEpoch = sessionEpoch
            record.failurePhase = "submitting"
            record.failureMessage = "上次提交结果尚未确认。为避免重复计费，客户端不会自动重发。"
            record.retryable = false
            transition(&record, to: .submissionUncertain)
            try save(record)
            return try execution(record, mode: .needsReview)
        }

        if record.status == .prepared {
            if record.sessionEpoch != sessionEpoch {
                record.sessionEpoch = sessionEpoch
                record.stateVersion += 1
                record.updatedAt = Date()
                try save(record)
            }
            return try execution(record, mode: .create, lease: executionLease)
        }
        if let providerJobID = record.providerJobID,
           record.status != .submissionUncertain,
           record.status != .cancelled {
            record.sessionEpoch = sessionEpoch
            record.retryCount += 1
            record.stateVersion += 1
            record.updatedAt = Date()
            try save(record)
            return try execution(
                record,
                mode: .resume(providerJobID: providerJobID),
                lease: executionLease
            )
        }
        return try execution(record, mode: .needsReview)
    }

    func snapshot(id: UUID, owner: String) throws -> Snapshot {
        let lockDescriptor = try acquireStoreLock()
        defer { releaseStoreLock(lockDescriptor) }
        return snapshot(try load(id: id, owner: owner))
    }

    /// Distinguishes an unpublished legacy Batch intent from a staged durable record left by
    /// a crash before the Batch handoff. Corrupt records still fail closed instead of being
    /// treated as absent and imported as a different lifecycle.
    func snapshotIfPresent(id: UUID, owner: String) throws -> Snapshot? {
        let lockDescriptor = try acquireStoreLock()
        defer { releaseStoreLock(lockDescriptor) }
        let manifest = jobFolder(id: id, owner: owner).appendingPathComponent("job.json")
        guard FileManager.default.fileExists(atPath: manifest.path) else { return nil }
        return snapshot(try load(id: id, owner: owner))
    }

    /// Removes only staging directories created by lease-aware versions of the store. The
    /// nonblocking lease check protects a slow copy in another App process; older lease-less
    /// directories are preserved because age alone cannot prove that their writer is dead.
    func removeExpiredStaging(
        owner ownerInput: String,
        olderThan cutoff: Date,
        limit: Int = MediaStudioVideoJobStore.maximumStagingCleanupCandidates
    ) throws -> Int {
        let owner = ownerInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !owner.isEmpty else { throw MediaStudioVideoJobStoreError.invalidOwner }
        guard limit > 0 else { return 0 }
        let directory = ownerDirectory(owner)
        guard FileManager.default.fileExists(atPath: directory.path) else { return 0 }
        let writer = try acquireStoreLock()
        defer { releaseStoreLock(writer) }
        guard let enumerator = FileManager.default.enumerator(
            at: directory,
            includingPropertiesForKeys: [
                .isDirectoryKey,
                .isSymbolicLinkKey,
                .contentModificationDateKey,
            ],
            options: [.skipsSubdirectoryDescendants]
        ) else { throw CocoaError(.fileReadUnknown) }
        var inspected = 0
        var removed = 0
        for case let candidate as URL in enumerator {
            let prefix = ".staging-"
            guard candidate.lastPathComponent.hasPrefix(prefix),
                  UUID(uuidString: String(candidate.lastPathComponent.dropFirst(prefix.count))) != nil
            else { continue }
            inspected += 1
            if inspected > limit { break }
            let values = try candidate.resourceValues(forKeys: [
                .isDirectoryKey,
                .isSymbolicLinkKey,
                .contentModificationDateKey,
            ])
            guard values.isDirectory == true,
                  values.isSymbolicLink != true,
                  let modifiedAt = values.contentModificationDate,
                  modifiedAt < cutoff else { continue }
            let lease = candidate.appendingPathComponent(Self.stagingLeaseFilename)
            let descriptor = lease.withUnsafeFileSystemRepresentation { path -> Int32 in
                guard let path else { return -1 }
                return Darwin.open(path, O_RDWR | O_NOFOLLOW | O_CLOEXEC)
            }
            guard descriptor >= 0 else { continue }
            var status = stat()
            guard fstat(descriptor, &status) == 0,
                  (status.st_mode & S_IFMT) == S_IFREG,
                  status.st_nlink == 1,
                  status.st_uid == geteuid() else {
                _ = Darwin.close(descriptor)
                continue
            }
            guard flock(descriptor, LOCK_EX | LOCK_NB) == 0 else {
                _ = Darwin.close(descriptor)
                continue
            }
            do {
                try FileManager.default.removeItem(at: candidate)
            } catch {
                releaseStagingLease(descriptor)
                throw error
            }
            releaseStagingLease(descriptor)
            removed += 1
        }
        return removed
    }

    private func maintainStaging(owner: String) {
        guard !maintainedStagingOwners.contains(owner) else { return }
        let cutoff = Date().addingTimeInterval(-Self.stagingRetention)
        guard (try? removeExpiredStaging(owner: owner, olderThan: cutoff)) != nil else { return }
        maintainedStagingOwners.insert(owner)
    }

    private func validateStagedIdentity(id: UUID, owner: String, workload: Workload) throws {
        guard !owner.isEmpty else { throw MediaStudioVideoJobStoreError.invalidOwner }
        guard workload.isValid, workload != .genericStudio else {
            throw MediaStudioVideoJobStoreError.invalidWorkload
        }
        guard workload.attemptID == id else {
            throw MediaStudioVideoJobStoreError.invalidWorkload
        }
    }

    private func makeStagedRecord(
        request: VideoGenerationRequest,
        id: UUID,
        owner: String,
        sessionEpoch: UUID,
        workload: Workload,
        activation: Activation,
        providerJobID: String?,
        status: Status,
        persistsInputs: Bool
    ) throws -> StagedRecord {
        let ownerFolder = ownerDirectory(owner)
        try createPrivateDirectory(ownerFolder)
        let folder = ownerFolder.appendingPathComponent(
            ".staging-\(UUID().uuidString.lowercased())",
            isDirectory: true
        )
        try createPrivateDirectory(folder)
        let leaseDescriptor: Int32
        do {
            leaseDescriptor = try acquireStagingLease(in: folder)
        } catch {
            try? FileManager.default.removeItem(at: folder)
            throw error
        }
        do {
            let storedRequest: StoredRequest
            if persistsInputs {
                let inputFolder = folder.appendingPathComponent("inputs", isDirectory: true)
                try createPrivateDirectory(inputFolder)
                storedRequest = try persist(request: request, in: inputFolder)
            } else {
                storedRequest = requestMetadata(request)
            }
            let now = Date()
            let record = Record(
                formatVersion: Self.formatVersion,
                id: id,
                ownerUserID: owner,
                workload: workload,
                activation: activation,
                sessionEpoch: sessionEpoch,
                idempotencyKey: "chatos-media-video-\(id.uuidString.lowercased())",
                providerJobID: providerJobID,
                status: status,
                retryCount: 0,
                stateVersion: 1,
                failurePhase: nil,
                failureMessage: nil,
                retryable: true,
                createdAt: now,
                updatedAt: now,
                request: storedRequest
            )
            try write(record, to: folder.appendingPathComponent(".job.json.staged"))
            return .init(
                record: record,
                folder: folder,
                leaseDescriptor: leaseDescriptor
            )
        } catch {
            releaseStagingLease(leaseDescriptor)
            try? FileManager.default.removeItem(at: folder)
            throw error
        }
    }

    /// Publishes a fully copied and hashed hidden staging directory with one same-volume
    /// rename. The writer lock covers collision resolution and the final visibility change,
    /// not the large input copy.
    private func publish(_ expected: Record, from stagingFolder: URL) throws -> Record {
        let lockDescriptor = try acquireStoreLock()
        defer { releaseStoreLock(lockDescriptor) }
        return try publishUnlocked(expected, from: stagingFolder)
    }

    private func publishUnlocked(_ expected: Record, from stagingFolder: URL) throws -> Record {
        let finalFolder = jobFolder(id: expected.id, owner: expected.ownerUserID)
        let finalManifest = finalFolder.appendingPathComponent("job.json")
        if FileManager.default.fileExists(atPath: finalManifest.path) {
            let existing = try decode(finalManifest)
            guard isIdempotentMatch(existing, expected: expected) else {
                throw MediaStudioVideoJobStoreError.jobConflict
            }
            return existing
        }
        guard !FileManager.default.fileExists(atPath: finalFolder.path) else {
            throw MediaStudioVideoJobStoreError.invalidRecord
        }
        try FileManager.default.removeItem(
            at: stagingFolder.appendingPathComponent(Self.stagingLeaseFilename)
        )
        let stagedManifest = stagingFolder.appendingPathComponent(".job.json.staged")
        try FileManager.default.moveItem(
            at: stagedManifest,
            to: stagingFolder.appendingPathComponent("job.json")
        )
        try FileManager.default.moveItem(at: stagingFolder, to: finalFolder)
        return expected
    }

    private func discard(_ staged: StagedRecord) {
        try? FileManager.default.removeItem(at: staged.folder)
        releaseStagingLease(staged.leaseDescriptor)
    }

    private func isIdempotentMatch(_ existing: Record, expected: Record) -> Bool {
        guard existing.id == expected.id,
              existing.ownerUserID == expected.ownerUserID,
              resolvedWorkload(existing) == resolvedWorkload(expected),
              existing.idempotencyKey == expected.idempotencyKey,
              existing.providerJobID == expected.providerJobID else { return false }
        if expected.providerJobID != nil { return existing.request == expected.request }
        return existing.status == expected.status && existing.request == expected.request
    }

    private func requestMetadata(_ request: VideoGenerationRequest) -> StoredRequest {
        .init(
            modelConfigID: request.modelConfigID,
            prompt: request.prompt,
            size: request.size,
            seconds: request.seconds,
            referencePurpose: referencePurpose(request.referencePurpose),
            ratio: request.ratio,
            inputs: []
        )
    }

    private func acquireStagingLease(in folder: URL) throws -> Int32 {
        let lease = folder.appendingPathComponent(Self.stagingLeaseFilename)
        let descriptor = lease.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(
                path,
                O_RDWR | O_CREAT | O_NOFOLLOW | O_CLOEXEC,
                mode_t(0o600)
            )
        }
        guard descriptor >= 0 else {
            throw MediaStudioVideoJobStoreError.invalidWriterLock
        }
        var shouldClose = true
        defer {
            if shouldClose { _ = Darwin.close(descriptor) }
        }
        var status = stat()
        guard fstat(descriptor, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              status.st_nlink == 1,
              status.st_uid == geteuid(),
              fchmod(descriptor, mode_t(0o600)) == 0 else {
            throw MediaStudioVideoJobStoreError.invalidWriterLock
        }
        while flock(descriptor, LOCK_EX) != 0 {
            guard errno == EINTR else {
                throw MediaStudioVideoJobStoreError.invalidWriterLock
            }
        }
        shouldClose = false
        return descriptor
    }

    private func releaseStagingLease(_ descriptor: Int32) {
        _ = flock(descriptor, LOCK_UN)
        _ = Darwin.close(descriptor)
    }

    private func acquireExecutionLeaseIfNeeded(
        id: UUID,
        owner: String,
        mode: RecoveryMode
    ) throws -> ExecutionLease? {
        switch mode {
        case .create, .resume:
            try acquireExecutionLease(id: id, owner: owner)
        case .needsReview:
            nil
        }
    }

    private func acquireExecutionLease(id: UUID, owner: String) throws -> ExecutionLease {
        let lease = jobFolder(id: id, owner: owner)
            .appendingPathComponent(Self.executionLeaseFilename)
        let descriptor = lease.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(
                path,
                O_RDWR | O_CREAT | O_NOFOLLOW | O_CLOEXEC,
                mode_t(0o600)
            )
        }
        guard descriptor >= 0 else {
            throw MediaStudioVideoJobStoreError.invalidWriterLock
        }
        var shouldClose = true
        defer {
            if shouldClose { _ = Darwin.close(descriptor) }
        }
        var status = stat()
        guard fstat(descriptor, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              status.st_nlink == 1,
              status.st_uid == geteuid(),
              fchmod(descriptor, mode_t(0o600)) == 0 else {
            throw MediaStudioVideoJobStoreError.invalidWriterLock
        }
        while flock(descriptor, LOCK_EX | LOCK_NB) != 0 {
            if errno == EINTR { continue }
            if errno == EWOULDBLOCK || errno == EAGAIN {
                throw MediaStudioVideoJobStoreError.jobBusy
            }
            throw MediaStudioVideoJobStoreError.invalidWriterLock
        }
        shouldClose = false
        return ExecutionLease(descriptor: descriptor)
    }

    /// Serializes a complete read/verify/transition/write operation across independent
    /// Store actors and processes. The persistent inode must never be unlinked to unlock;
    /// `flock` ownership is tied to this descriptor and the kernel releases it on crash.
    private func acquireStoreLock() throws -> Int32 {
        try createPrivateDirectory(root)
        let lockURL = root.appendingPathComponent(".writer.lock", isDirectory: false)
        let descriptor = lockURL.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(
                path,
                O_RDWR | O_CREAT | O_NOFOLLOW | O_CLOEXEC,
                mode_t(0o600)
            )
        }
        guard descriptor >= 0 else {
            throw MediaStudioVideoJobStoreError.invalidWriterLock
        }
        var shouldClose = true
        defer {
            if shouldClose { Darwin.close(descriptor) }
        }
        var status = stat()
        guard fstat(descriptor, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              status.st_nlink == 1,
              status.st_uid == geteuid(),
              fchmod(descriptor, mode_t(0o600)) == 0 else {
            throw MediaStudioVideoJobStoreError.invalidWriterLock
        }
        while flock(descriptor, LOCK_EX) != 0 {
            guard errno == EINTR else {
                throw MediaStudioVideoJobStoreError.invalidWriterLock
            }
        }
        shouldClose = false
        return descriptor
    }

    private func releaseStoreLock(_ descriptor: Int32) {
        _ = flock(descriptor, LOCK_UN)
        _ = Darwin.close(descriptor)
    }

    private func persist(request: VideoGenerationRequest, in folder: URL) throws -> StoredRequest {
        var inputs: [InputArtifact] = []
        if let value = request.inputImage {
            inputs.append(try persist(
                kind: .firstFrame, name: value.name, mimeType: value.mimeType,
                fileURL: value.fileURL, base64Data: value.base64Data, in: folder
            ))
        }
        if let value = request.lastFrameImage {
            inputs.append(try persist(
                kind: .lastFrame, name: value.name, mimeType: value.mimeType,
                fileURL: value.fileURL, base64Data: value.base64Data, in: folder
            ))
        }
        if let value = request.referenceVideo {
            inputs.append(try persist(
                kind: .referenceVideo, name: value.name, mimeType: value.mimeType,
                fileURL: value.fileURL, base64Data: value.base64Data, in: folder
            ))
        }
        if let value = request.referenceAudio {
            inputs.append(try persist(
                kind: .referenceAudio, name: value.name, mimeType: value.mimeType,
                fileURL: value.fileURL, base64Data: value.base64Data, in: folder
            ))
        }
        return StoredRequest(
            modelConfigID: request.modelConfigID,
            prompt: request.prompt,
            size: request.size,
            seconds: request.seconds,
            referencePurpose: referencePurpose(request.referencePurpose),
            ratio: request.ratio,
            inputs: inputs
        )
    }

    private func persist(
        kind: InputKind,
        name: String,
        mimeType: String,
        fileURL: URL?,
        base64Data: String,
        in folder: URL
    ) throws -> InputArtifact {
        let filename = "\(kind.rawValue).artifact"
        let destination = folder.appendingPathComponent(filename)
        let byteCount: Int
        if let fileURL {
            guard base64Data.isEmpty else { throw MediaStudioVideoJobStoreError.invalidInput }
            let values = try fileURL.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
            guard values.isRegularFile == true,
                  let fileSize = values.fileSize,
                  fileSize > 0,
                  fileSize <= kind.maximumBytes else {
                throw MediaStudioVideoJobStoreError.invalidInput
            }
            try AppBoundedFileReader.copy(
                fileURL,
                to: destination,
                expectedBytes: fileSize,
                maximumBytes: kind.maximumBytes
            )
            byteCount = fileSize
        } else {
            guard let data = Data(base64Encoded: base64Data),
                  !data.isEmpty,
                  data.count <= kind.maximumBytes else {
                throw MediaStudioVideoJobStoreError.invalidInput
            }
            try data.write(to: destination, options: [.atomic])
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o600],
                ofItemAtPath: destination.path
            )
            byteCount = data.count
        }
        return .init(
            kind: kind,
            filename: filename,
            originalName: name,
            mimeType: mimeType,
            byteCount: byteCount,
            sha256: try digest(destination, expectedBytes: byteCount)
        )
    }

    private func execution(
        _ record: Record,
        mode: RecoveryMode,
        validatesInputs: Bool = true,
        lease: ExecutionLease? = nil
    ) throws -> Execution {
        let folder = jobFolder(id: record.id, owner: record.ownerUserID)
            .appendingPathComponent("inputs", isDirectory: true)
        var request = VideoGenerationRequest(
            clientRequestID: record.idempotencyKey,
            modelConfigID: record.request.modelConfigID,
            prompt: record.request.prompt,
            size: record.request.size,
            seconds: record.request.seconds,
            referencePurpose: try referencePurpose(record.request.referencePurpose),
            ratio: record.request.ratio
        )
        for input in mode == .create ? record.request.inputs : [] {
            let url = folder.appendingPathComponent(input.filename)
            if validatesInputs {
                guard try digest(url, expectedBytes: input.byteCount) == input.sha256 else {
                    throw MediaStudioVideoJobStoreError.invalidInput
                }
            }
            switch input.kind {
            case .firstFrame:
                request.inputImage = .init(
                    name: input.originalName, mimeType: input.mimeType, fileURL: url
                )
            case .lastFrame:
                request.lastFrameImage = .init(
                    name: input.originalName, mimeType: input.mimeType, fileURL: url
                )
            case .referenceVideo:
                request.referenceVideo = .init(
                    name: input.originalName, mimeType: input.mimeType, fileURL: url
                )
            case .referenceAudio:
                request.referenceAudio = .init(
                    name: input.originalName, mimeType: input.mimeType, fileURL: url
                )
            }
        }
        return .init(snapshot: snapshot(record), request: request, mode: mode, lease: lease)
    }

    private func loadRecords(owner: String) throws -> [Record] {
        let directory = ownerDirectory(owner)
        guard FileManager.default.fileExists(atPath: directory.path) else { return [] }
        let children = try FileManager.default.contentsOfDirectory(
            at: directory,
            includingPropertiesForKeys: [.isDirectoryKey],
            options: [.skipsHiddenFiles]
        )
        var records: [Record] = []
        for folder in children {
            guard (try? folder.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true
            else { continue }
            let manifest = folder.appendingPathComponent("job.json")
            // A process death before the first atomic manifest commit leaves only an
            // uncommitted artifact folder and therefore cannot have reached the provider.
            guard FileManager.default.fileExists(atPath: manifest.path) else { continue }
            records.append(try decode(manifest))
        }
        return records
    }

    private func load(id: UUID, owner: String) throws -> Record {
        let record = try decode(jobFolder(id: id, owner: owner).appendingPathComponent("job.json"))
        guard record.id == id, record.ownerUserID == owner else {
            throw MediaStudioVideoJobStoreError.invalidRecord
        }
        return record
    }

    private func loadActive(id: UUID, owner: String) throws -> Record {
        let record = try load(id: id, owner: owner)
        guard resolvedActivation(record) == .active else {
            throw MediaStudioVideoJobStoreError.activationRequired
        }
        return record
    }

    private func decode(_ url: URL) throws -> Record {
        let record = try JSONDecoder().decode(
            Record.self,
            from: AppBoundedFileReader.read(url, maximumBytes: Self.maximumManifestBytes)
        )
        let hasVersionedWorkload = record.formatVersion == 1
            ? record.workload == nil
            : record.workload != nil
        let hasVersionedActivation = record.formatVersion < 3
            ? record.activation == nil
            : record.activation != nil
        guard (1...Self.formatVersion).contains(record.formatVersion),
              hasVersionedWorkload,
              hasVersionedActivation,
              record.stateVersion > 0,
              record.retryCount >= 0,
              record.workload.map(\.isValid) ?? true,
              !record.idempotencyKey.isEmpty,
              Set(record.request.inputs.map(\.kind)).count == record.request.inputs.count,
              record.request.inputs.allSatisfy({ input in
                  input.filename == "\(input.kind.rawValue).artifact"
                      && input.byteCount > 0
                      && input.byteCount <= input.kind.maximumBytes
                      && input.sha256.count == 64
                      && !input.originalName.isEmpty
                      && !input.mimeType.isEmpty
              }) else {
            throw MediaStudioVideoJobStoreError.invalidRecord
        }
        return record
    }

    private func save(_ record: Record) throws {
        let folder = jobFolder(id: record.id, owner: record.ownerUserID)
        try createPrivateDirectory(folder)
        try write(record, to: folder.appendingPathComponent("job.json"))
    }

    private func write(_ record: Record, to manifest: URL) throws {
        let data = try JSONEncoder().encode(record)
        guard data.count <= Self.maximumManifestBytes else {
            throw MediaStudioVideoJobStoreError.invalidRecord
        }
        try data.write(to: manifest, options: [.atomic])
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o600],
            ofItemAtPath: manifest.path
        )
    }

    private func removeInputs(for record: inout Record) throws {
        guard !record.request.inputs.isEmpty else { return }
        try? FileManager.default.removeItem(
            at: jobFolder(id: record.id, owner: record.ownerUserID)
                .appendingPathComponent("inputs", isDirectory: true)
        )
        record.request.inputs = []
    }

    private func transition(_ record: inout Record, to status: Status) {
        record.status = status
        record.stateVersion += 1
        record.updatedAt = Date()
    }

    private func verify(sessionEpoch: UUID?, record: Record) throws {
        if let sessionEpoch, sessionEpoch != record.sessionEpoch {
            throw MediaStudioVideoJobStoreError.staleSession
        }
    }

    private static func requiresRecovery(_ record: Record) -> Bool {
        switch record.status {
        case .completed, .cancelled:
            false
        case .failed:
            record.providerJobID != nil && record.retryable && record.retryCount < 3
        default:
            true
        }
    }

    private static func mayHaveCrossedSubmissionBoundary(_ status: Status) -> Bool {
        switch status {
        case .submitting, .submitted, .running, .downloading, .validating,
             .finalizing, .cancellationRequested:
            true
        case .prepared, .completed, .submissionUncertain, .cancelled, .failed:
            false
        }
    }

    private func digest(_ url: URL, expectedBytes: Int) throws -> String {
        let descriptor = url.withUnsafeFileSystemRepresentation { path -> Int32 in
            guard let path else { return -1 }
            return Darwin.open(path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { throw MediaStudioVideoJobStoreError.invalidInput }
        let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        defer { try? handle.close() }
        var status = stat()
        guard fstat(descriptor, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              status.st_size == Int64(expectedBytes) else {
            throw MediaStudioVideoJobStoreError.invalidInput
        }
        var hasher = SHA256()
        var remaining = expectedBytes
        while remaining > 0 {
            let chunk = try handle.read(upToCount: min(256 * 1_024, remaining)) ?? Data()
            guard !chunk.isEmpty else { throw MediaStudioVideoJobStoreError.invalidInput }
            hasher.update(data: chunk)
            remaining -= chunk.count
        }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }

    private func snapshot(_ record: Record) -> Snapshot {
        .init(
            id: record.id,
            ownerUserID: record.ownerUserID,
            workload: resolvedWorkload(record),
            isActive: resolvedActivation(record) == .active,
            sessionEpoch: record.sessionEpoch,
            idempotencyKey: record.idempotencyKey,
            providerJobID: record.providerJobID,
            status: record.status,
            retryCount: record.retryCount,
            stateVersion: record.stateVersion,
            failurePhase: record.failurePhase,
            failureMessage: record.failureMessage,
            retryable: record.retryable,
            updatedAt: record.updatedAt
        )
    }

    private func resolvedWorkload(_ record: Record) -> Workload {
        record.workload ?? .genericStudio
    }

    private func resolvedActivation(_ record: Record) -> Activation {
        record.activation ?? .active
    }

    private func recoveryMode(_ record: Record) -> RecoveryMode {
        if record.status == .prepared { return .create }
        if let providerJobID = record.providerJobID,
           Self.requiresRecovery(record),
           record.status != .submissionUncertain,
           record.status != .cancelled {
            return .resume(providerJobID: providerJobID)
        }
        return .needsReview
    }

    private func ownerDirectory(_ owner: String) -> URL {
        let key = SHA256.hash(data: Data(owner.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
        return root.appendingPathComponent("Accounts/\(key)", isDirectory: true)
    }

    private func jobFolder(id: UUID, owner: String) -> URL {
        ownerDirectory(owner).appendingPathComponent(id.uuidString.lowercased(), isDirectory: true)
    }

    private func createPrivateDirectory(_ url: URL) throws {
        try FileManager.default.createDirectory(
            at: url,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: url.path
        )
    }

    private func referencePurpose(_ value: VideoGenerationReferencePurpose) -> String {
        switch value {
        case .reference: "reference"
        case .edit: "edit"
        case .extend: "extend"
        }
    }

    private func referencePurpose(_ value: String) throws -> VideoGenerationReferencePurpose {
        switch value {
        case "reference": .reference
        case "edit": .edit
        case "extend": .extend
        default: throw MediaStudioVideoJobStoreError.invalidRecord
        }
    }
}

enum MediaStudioVideoJobStoreError: LocalizedError {
    case invalidOwner
    case invalidInput
    case invalidRecord
    case invalidTransition
    case staleSession
    case invalidWriterLock
    case invalidWorkload
    case invalidProviderJob
    case activationRequired
    case jobConflict
    case jobBusy

    var errorDescription: String? {
        switch self {
        case .invalidOwner: "视频任务缺少有效的账号上下文。"
        case .invalidInput: "视频任务输入文件无效或超过大小限制。"
        case .invalidRecord: "视频任务记录损坏，无法安全恢复。"
        case .invalidTransition: "视频任务状态已变化，请刷新后重试。"
        case .staleSession: "视频任务已由新的账号会话接管。"
        case .invalidWriterLock: "视频任务写入锁无效，已拒绝继续操作。"
        case .invalidWorkload: "视频任务缺少有效的业务关联。"
        case .invalidProviderJob: "视频任务缺少有效的服务商任务 ID。"
        case .activationRequired: "视频任务尚未完成业务所有权切换。"
        case .jobConflict: "视频任务 ID 已关联到不同的请求，已拒绝覆盖。"
        case .jobBusy: "视频任务正在另一个客户端进程中执行，请稍后再试。"
        }
    }
}
