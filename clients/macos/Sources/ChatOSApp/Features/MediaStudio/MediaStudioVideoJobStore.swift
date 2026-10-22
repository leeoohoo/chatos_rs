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

    struct Snapshot: Sendable, Equatable {
        var id: UUID
        var ownerUserID: String
        var workload: Workload
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

    private struct InputArtifact: Codable {
        var kind: InputKind
        var filename: String
        var originalName: String
        var mimeType: String
        var byteCount: Int
        var sha256: String
    }

    private struct StoredRequest: Codable {
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

    private static let formatVersion = 2
    private static let maximumManifestBytes = 1 * 1_024 * 1_024
    private let root: URL

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
        let id = UUID()
        let folder = jobFolder(id: id, owner: owner)
        let inputFolder = folder.appendingPathComponent("inputs", isDirectory: true)
        var committed = false
        defer {
            if !committed { try? FileManager.default.removeItem(at: folder) }
        }
        try createPrivateDirectory(ownerDirectory(owner))
        try createPrivateDirectory(inputFolder)
        let storedRequest = try persist(request: request, in: inputFolder)
        let now = Date()
        let record = Record(
            formatVersion: Self.formatVersion,
            id: id,
            ownerUserID: owner,
            workload: workload,
            sessionEpoch: sessionEpoch,
            idempotencyKey: "chatos-media-video-\(id.uuidString.lowercased())",
            providerJobID: nil,
            status: .prepared,
            retryCount: 0,
            stateVersion: 1,
            failurePhase: nil,
            failureMessage: nil,
            retryable: true,
            createdAt: now,
            updatedAt: now,
            request: storedRequest
        )
        // This folder has no manifest yet, so recovery treats it as uncommitted staging.
        // Copying and hashing a large input does not need the global writer lock. Only the
        // atomic manifest commit and publication of its final URLs are serialized.
        let lockDescriptor = try acquireStoreLock()
        defer { releaseStoreLock(lockDescriptor) }
        do {
            try save(record)
            let prepared = try execution(record, mode: .create, validatesInputs: false)
            committed = true
            return prepared
        } catch {
            // Keep a failed manifest commit invisible to another locked reader.
            try? FileManager.default.removeItem(at: folder)
            throw error
        }
    }

    func markSubmitting(id: UUID, owner: String, sessionEpoch: UUID) throws -> Execution {
        let lockDescriptor = try acquireStoreLock()
        defer { releaseStoreLock(lockDescriptor) }
        var record = try load(id: id, owner: owner)
        guard record.status == .prepared else {
            throw MediaStudioVideoJobStoreError.invalidTransition
        }
        record.sessionEpoch = sessionEpoch
        transition(&record, to: .submitting)
        try save(record)
        return try execution(record, mode: .create)
    }

    func recordProgress(
        id: UUID,
        owner: String,
        sessionEpoch: UUID? = nil,
        progress: VideoGenerationProgress
    ) throws {
        let lockDescriptor = try acquireStoreLock()
        defer { releaseStoreLock(lockDescriptor) }
        var record = try load(id: id, owner: owner)
        try verify(sessionEpoch: sessionEpoch, record: record)
        guard !record.status.isTerminal else { return }
        let hadProviderJobID = record.providerJobID != nil
        if let providerJobID = progress.jobID?.trimmingCharacters(in: .whitespacesAndNewlines),
           !providerJobID.isEmpty {
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
        let lockDescriptor = try acquireStoreLock()
        defer { releaseStoreLock(lockDescriptor) }
        var record = try load(id: id, owner: owner)
        try verify(sessionEpoch: sessionEpoch, record: record)
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
        let lockDescriptor = try acquireStoreLock()
        defer { releaseStoreLock(lockDescriptor) }
        var record = try load(id: id, owner: owner)
        try verify(sessionEpoch: sessionEpoch, record: record)
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
        var record = try load(id: id, owner: owner)
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
        var record = try load(id: id, owner: owner)
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
        let lockDescriptor = try acquireStoreLock()
        defer { releaseStoreLock(lockDescriptor) }
        let normalizedOwner = owner.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalizedOwner.isEmpty else { throw MediaStudioVideoJobStoreError.invalidOwner }
        guard workload.isValid else { throw MediaStudioVideoJobStoreError.invalidWorkload }
        let records = try loadRecords(owner: normalizedOwner)
            .filter { resolvedWorkload($0) == workload }
            .filter(Self.requiresRecovery)
            .sorted { $0.updatedAt > $1.updatedAt }
        guard var record = records.first else { return nil }
        record.sessionEpoch = sessionEpoch

        if record.providerJobID == nil && Self.mayHaveCrossedSubmissionBoundary(record.status) {
            record.failurePhase = "submitting"
            record.failureMessage = "上次提交结果尚未确认。为避免重复计费，客户端不会自动重发。"
            record.retryable = false
            transition(&record, to: .submissionUncertain)
            try save(record)
        }

        let mode: RecoveryMode
        if record.status == .prepared {
            mode = .create
        } else if let providerJobID = record.providerJobID,
                  record.status != .submissionUncertain,
                  record.status != .cancelled {
            record.retryCount += 1
            record.stateVersion += 1
            record.updatedAt = Date()
            try save(record)
            mode = .resume(providerJobID: providerJobID)
        } else {
            mode = .needsReview
        }
        return try execution(record, mode: mode)
    }

    func snapshot(id: UUID, owner: String) throws -> Snapshot {
        let lockDescriptor = try acquireStoreLock()
        defer { releaseStoreLock(lockDescriptor) }
        return snapshot(try load(id: id, owner: owner))
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
        validatesInputs: Bool = true
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
        return .init(snapshot: snapshot(record), request: request, mode: mode)
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

    private func decode(_ url: URL) throws -> Record {
        let record = try JSONDecoder().decode(
            Record.self,
            from: AppBoundedFileReader.read(url, maximumBytes: Self.maximumManifestBytes)
        )
        let hasVersionedWorkload = record.formatVersion == 1
            ? record.workload == nil
            : record.workload != nil
        guard (1...Self.formatVersion).contains(record.formatVersion),
              hasVersionedWorkload,
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
        let data = try JSONEncoder().encode(record)
        guard data.count <= Self.maximumManifestBytes else {
            throw MediaStudioVideoJobStoreError.invalidRecord
        }
        let manifest = folder.appendingPathComponent("job.json")
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

    var errorDescription: String? {
        switch self {
        case .invalidOwner: "视频任务缺少有效的账号上下文。"
        case .invalidInput: "视频任务输入文件无效或超过大小限制。"
        case .invalidRecord: "视频任务记录损坏，无法安全恢复。"
        case .invalidTransition: "视频任务状态已变化，请刷新后重试。"
        case .staleSession: "视频任务已由新的账号会话接管。"
        case .invalidWriterLock: "视频任务写入锁无效，已拒绝继续操作。"
        case .invalidWorkload: "视频任务缺少有效的业务关联。"
        }
    }
}
