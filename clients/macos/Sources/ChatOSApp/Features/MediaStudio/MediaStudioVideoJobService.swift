import ChatOSCore
import Foundation

protocol MediaStudioDurableVideoHistoryPersisting: MediaStudioVideoHistoryPersisting {
    func saveVideo(
        _ result: VideoGenerationResult,
        prompt: String,
        owner: String,
        durableRecordID: String
    ) async throws -> MediaStudioVideoHistoryItem
}

/// In-process owner of the generic Media Studio video lifecycle. The ViewModel observes this
/// service but never advances durable job state itself.
actor MediaStudioVideoJobService {
    enum RecoveryResult: Sendable {
        case none
        case needsReview(progress: VideoGenerationProgress, message: String)
        case finished(MediaStudioVideoGenerationOutcome)
    }

    private let generator: any MediaGenerationServicing
    private let history: any MediaStudioDurableVideoHistoryPersisting
    private let store: MediaStudioVideoJobStore

    init(
        generator: any MediaGenerationServicing,
        history: any MediaStudioDurableVideoHistoryPersisting,
        store: MediaStudioVideoJobStore
    ) {
        self.generator = generator
        self.history = history
        self.store = store
    }

    func submit(
        request: VideoGenerationRequest,
        owner: String,
        sessionEpoch: UUID,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> MediaStudioVideoGenerationOutcome {
        let execution = try await store.prepare(
            request: request,
            owner: owner,
            sessionEpoch: sessionEpoch
        )
        return try await execute(
            execution,
            owner: owner,
            sessionEpoch: sessionEpoch,
            progress: progress
        )
    }

    func recover(
        owner: String,
        sessionEpoch: UUID,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> RecoveryResult {
        guard let execution = try await store.recover(
            owner: owner,
            sessionEpoch: sessionEpoch
        ) else { return .none }
        switch execution.mode {
        case .needsReview:
            return .needsReview(
                progress: .init(
                    status: execution.snapshot.status.rawValue,
                    jobID: execution.snapshot.providerJobID
                ),
                message: execution.snapshot.failureMessage
                    ?? "视频任务需要人工核对，客户端不会自动重新提交。"
            )
        case .create, .resume:
            return .finished(try await execute(
                execution,
                owner: owner,
                sessionEpoch: sessionEpoch,
                progress: progress
            ))
        }
    }

    private func execute(
        _ initialExecution: MediaStudioVideoJobStore.Execution,
        owner: String,
        sessionEpoch: UUID,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> MediaStudioVideoGenerationOutcome {
        var execution = initialExecution
        let service: any MediaGenerationServicing
        do {
            if let bindable = generator as? any SessionBoundMediaGenerationServicing {
                service = try await bindable.boundToCurrentSession()
            } else {
                service = generator
            }
        } catch {
            try? await store.markFailure(
                id: execution.snapshot.id,
                owner: owner,
                sessionEpoch: sessionEpoch,
                phase: "authentication",
                message: error.localizedDescription,
                requestMayHaveBeenSubmitted: false,
                retryable: true
            )
            throw error
        }

        do {
            switch execution.mode {
            case .create:
                execution = try await store.markSubmitting(
                    execution,
                    owner: owner,
                    sessionEpoch: sessionEpoch
                )
                await progress(.init(status: "submitting"))
            case let .resume(providerJobID):
                await progress(.init(status: "checking", jobID: providerJobID))
            case .needsReview:
                throw MediaStudioVideoJobServiceError.unresolvedSubmission
            }

            let gate = MediaStudioVideoProgressPersistenceGate()
            let store = self.store
            let localJobID = execution.snapshot.id
            let request = execution.request
            let result: VideoGenerationResult
            do {
                switch execution.mode {
                case .create:
                    result = try await service.generateVideo(request) { value in
                        do {
                            try await store.recordProgress(
                                id: localJobID,
                                owner: owner,
                                sessionEpoch: sessionEpoch,
                                progress: value
                            )
                            await progress(value)
                        } catch {
                            await gate.fail(error)
                            withUnsafeCurrentTask { $0?.cancel() }
                        }
                    }
                case let .resume(providerJobID):
                    guard let resumable = service as? any ResumableVideoGenerationServicing else {
                        throw MediaStudioVideoJobServiceError.resumeUnavailable
                    }
                    result = try await resumable.resumeVideo(
                        request,
                        jobID: providerJobID
                    ) { value in
                        do {
                            try await store.recordProgress(
                                id: localJobID,
                                owner: owner,
                                sessionEpoch: sessionEpoch,
                                progress: value
                            )
                            await progress(value)
                        } catch {
                            await gate.fail(error)
                            withUnsafeCurrentTask { $0?.cancel() }
                        }
                    }
                case .needsReview:
                    throw MediaStudioVideoJobServiceError.unresolvedSubmission
                }
            } catch {
                if let persistenceError = await gate.error { throw persistenceError }
                throw error
            }
            if let persistenceError = await gate.error {
                throw persistenceError
            }
            try Task.checkCancellation()
            defer { result.videoFile?.removeIfNeeded() }
            try await store.markFinalizing(
                id: localJobID,
                owner: owner,
                sessionEpoch: sessionEpoch,
                providerJobID: result.id
            )
            await progress(.init(status: "saving", jobID: result.id))
            do {
                let item = try await history.saveVideo(
                    result,
                    prompt: request.prompt,
                    owner: owner,
                    durableRecordID: localJobID.uuidString.lowercased()
                )
                try Task.checkCancellation()
                try await store.markCompleted(
                    id: localJobID,
                    owner: owner,
                    sessionEpoch: sessionEpoch,
                    providerJobID: result.id
                )
                return .saved(item)
            } catch is CancellationError {
                throw CancellationError()
            } catch {
                try? await store.markFailure(
                    id: localJobID,
                    owner: owner,
                    sessionEpoch: sessionEpoch,
                    phase: "finalizing",
                    message: error.localizedDescription,
                    requestMayHaveBeenSubmitted: true,
                    retryable: true
                )
                return .generatedWithoutPersistence(
                    persistenceErrorMessage: error.localizedDescription
                )
            }
        } catch is CancellationError {
            try? await store.markCancellation(
                id: execution.snapshot.id,
                owner: owner,
                sessionEpoch: sessionEpoch
            )
            throw CancellationError()
        } catch {
            let mayHaveSubmitted = (error as? any MediaGenerationSubmissionFailure)?
                .requestMayHaveBeenSubmitted ?? true
            do {
                try await store.markFailure(
                    id: execution.snapshot.id,
                    owner: owner,
                    sessionEpoch: sessionEpoch,
                    phase: phase(for: execution),
                    message: error.localizedDescription,
                    requestMayHaveBeenSubmitted: mayHaveSubmitted,
                    retryable: true
                )
                let snapshot = try await store.snapshot(
                    id: execution.snapshot.id,
                    owner: owner
                )
                if snapshot.status == .submissionUncertain {
                    throw MediaStudioVideoJobServiceError.unresolvedSubmission
                }
            } catch let reconciliationError as MediaStudioVideoJobServiceError {
                throw reconciliationError
            } catch {
                // The original operation error remains the most useful UI message. A failed
                // state write leaves the last durable state conservative on next launch.
            }
            throw error
        }
    }

    private func phase(for execution: MediaStudioVideoJobStore.Execution) -> String {
        switch execution.mode {
        case .create: "submitting"
        case .resume: "running"
        case .needsReview: "reconciliation"
        }
    }
}

private actor MediaStudioVideoProgressPersistenceGate {
    private(set) var error: (any Error)?

    func fail(_ error: any Error) {
        if self.error == nil { self.error = error }
    }
}

enum MediaStudioVideoJobServiceError: LocalizedError, Equatable {
    case unresolvedSubmission
    case resumeUnavailable

    var errorDescription: String? {
        switch self {
        case .unresolvedSubmission:
            "上次视频提交结果尚未确认。为避免重复计费，客户端不会自动重发。"
        case .resumeUnavailable:
            "当前视频服务无法恢复已有的服务商任务。"
        }
    }
}

/// Resolves terminal-job references without scanning unrelated history. Every workload carries
/// the exact identifiers needed to query its canonical business manifests.
struct MediaStudioVideoJobMaintenance: Sendable {
    let jobs: MediaStudioVideoJobStore
    let history: MediaStudioHistoryStore
    let stories: StoryProjectStore

    func run(
        owner: String,
        policy: MediaStudioVideoJobStore.TerminalMaintenancePolicy = .init()
    ) async throws -> MediaStudioVideoJobStore.TerminalMaintenanceReport {
        try await jobs.enforceTerminalBudget(owner: owner, policy: policy) { snapshot in
            switch snapshot.workload.kind {
            case .genericStudio:
                return try await history.referencesDurableVideoJob(
                    id: snapshot.id,
                    owner: owner
                )
            case .storyVideo, .storyBatchVideo:
                guard let projectID = snapshot.workload.projectID,
                      let segmentID = snapshot.workload.segmentID else {
                    throw MediaStudioVideoJobStoreError.invalidWorkload
                }
                return try await stories.referencesVideoJob(
                    id: snapshot.id,
                    owner: owner,
                    projectID: projectID,
                    segmentID: segmentID,
                    batchID: snapshot.workload.batchID
                )
            }
        }
    }
}
