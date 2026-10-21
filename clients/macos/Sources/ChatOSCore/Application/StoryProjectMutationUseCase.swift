import Foundation

public protocol StoryProjectMutationPersisting: Sendable {
    func persistStoryProject(
        _ project: StoryProject,
        ownerUserID: String,
        preservingVideoSegmentIDs: Set<String>
    ) async throws -> StoryProject
}

public struct StoryProjectSettingsChange: Equatable, Sendable {
    public var projectID: UUID
    public var title: String
    public var description: String
    public var models: StoryModelSelection

    public init(
        projectID: UUID,
        title: String,
        description: String,
        models: StoryModelSelection
    ) {
        self.projectID = projectID
        self.title = title
        self.description = description
        self.models = models
    }
}

/// Application boundary for creating a Story project and changing its settings.
///
/// These mutations only validate and persist the project manifest. They never call a
/// text, image, or video provider. Settings changes are deliberately reconstructed from
/// the current canonical project so a stale form cannot replace source text, plans,
/// generated media, job identities, or schema metadata.
public struct StoryProjectMutationUseCase: Sendable {
    private let persistence: any StoryProjectMutationPersisting

    public init(persistence: any StoryProjectMutationPersisting) {
        self.persistence = persistence
    }

    public func create(
        _ draft: StoryProject,
        ownerUserID ownerInput: String,
        availableModels: [MediaGenerationModel]
    ) async throws -> StoryProject {
        let owner = try validatedOwner(ownerInput)
        var next = draft
        next.models = try validatedModelSelection(draft.models, available: availableModels)
        try next.validate()
        let saved = try await persistence.persistStoryProject(
            next,
            ownerUserID: owner,
            preservingVideoSegmentIDs: []
        )
        try Task.checkCancellation()
        return saved
    }

    public func updateSettings(
        current: StoryProject,
        change: StoryProjectSettingsChange,
        ownerUserID ownerInput: String,
        availableModels: [MediaGenerationModel],
        preservingVideoSegmentIDs: Set<String>
    ) async throws -> StoryProject {
        let owner = try validatedOwner(ownerInput)
        guard change.projectID == current.id else {
            throw StoryProjectMutationError.projectMismatch
        }
        let models = try validatedModelSelection(change.models, available: availableModels)
        guard !current.hasUnresolvedVideoJobs
                || models.videoModelID == current.models.videoModelID,
              !current.hasUnresolvedImageJobs
                || models.imageModelID == current.models.imageModelID else {
            throw StoryError.unresolvedSubmission
        }

        var next = current
        next.title = change.title.trimmingCharacters(in: .whitespacesAndNewlines)
        next.description = change.description
        next.models = models
        try next.validate()
        let saved = try await persistence.persistStoryProject(
            next,
            ownerUserID: owner,
            preservingVideoSegmentIDs: preservingVideoSegmentIDs
        )
        try Task.checkCancellation()
        return saved
    }

    private func validatedOwner(_ ownerInput: String) throws -> String {
        let owner = ownerInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !owner.isEmpty else { throw StoryProjectMutationError.invalidOwner }
        return owner
    }

    private func validatedModelSelection(
        _ selection: StoryModelSelection,
        available: [MediaGenerationModel]
    ) throws -> StoryModelSelection {
        let eligibleIDs = Set(available.lazy.filter { $0.enabled && $0.hasAPIKey }.map(\.id))
        guard [selection.textModelID, selection.imageModelID, selection.videoModelID]
            .allSatisfy(eligibleIDs.contains) else {
            throw StoryError.missingModel
        }
        var result = selection
        if let model = available.first(where: { $0.id == selection.videoModelID }) {
            result.supportedVideoDurations = VideoGenerationProfile(
                modelName: model.modelName
            ).durations
        }
        return result
    }
}

public enum StoryProjectMutationError: LocalizedError, Equatable, Sendable {
    case invalidOwner
    case projectMismatch

    public var errorDescription: String? {
        switch self {
        case .invalidOwner:
            "剧情项目缺少有效的账号上下文。"
        case .projectMismatch:
            "剧情设置已过期，请重新打开后再保存。"
        }
    }
}

public enum StoryProjectMutationKind: Equatable, Sendable {
    case create(projectID: UUID)
    case updateSettings(projectID: UUID)
}

public struct StoryProjectMutationToken: Equatable, Sendable {
    public let ownerUserID: String
    public let accountSessionID: UUID
    public let requestID: UUID
    public let kind: StoryProjectMutationKind

    public init(
        ownerUserID: String,
        accountSessionID: UUID,
        requestID: UUID,
        kind: StoryProjectMutationKind
    ) {
        self.ownerUserID = ownerUserID
        self.accountSessionID = accountSessionID
        self.requestID = requestID
        self.kind = kind
    }
}

public struct StoryProjectMutationState: Equatable, Sendable {
    public private(set) var activeToken: StoryProjectMutationToken?

    public init() {}

    public mutating func begin(
        ownerUserID: String,
        accountSessionID: UUID,
        kind: StoryProjectMutationKind
    ) -> StoryProjectMutationToken? {
        guard activeToken == nil else { return nil }
        let token = StoryProjectMutationToken(
            ownerUserID: ownerUserID,
            accountSessionID: accountSessionID,
            requestID: UUID(),
            kind: kind
        )
        activeToken = token
        return token
    }

    public func accepts(
        _ token: StoryProjectMutationToken,
        ownerUserID: String?,
        accountSessionID: UUID
    ) -> Bool {
        activeToken == token
            && token.ownerUserID == ownerUserID
            && token.accountSessionID == accountSessionID
    }

    @discardableResult
    public mutating func finish(_ token: StoryProjectMutationToken) -> Bool {
        guard activeToken == token else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}
