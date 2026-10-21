import Foundation

public struct StoryDirectVideoQueuePlan: Equatable, Sendable {
    public let queueID: UUID
    public let ownerUserID: String
    public let projectID: UUID
    public let projectFingerprint: String
    public let targetIDs: [String]
    public let model: MediaGenerationModel
    public let size: String
    public let userIdeas: String

    public init(
        queueID: UUID,
        ownerUserID: String,
        projectID: UUID,
        projectFingerprint: String,
        targetIDs: [String],
        model: MediaGenerationModel,
        size: String,
        userIdeas: String
    ) {
        self.queueID = queueID
        self.ownerUserID = ownerUserID
        self.projectID = projectID
        self.projectFingerprint = projectFingerprint
        self.targetIDs = targetIDs
        self.model = model
        self.size = size
        self.userIdeas = userIdeas
    }
}

public struct StoryDirectVideoQueueStep: Equatable, Sendable {
    public let queueID: UUID
    public let projectID: UUID
    public let segmentID: String
    public let index: Int
    public let totalCount: Int
    public let model: MediaGenerationModel
    public let size: String
    public let userIdeas: String

    public init(
        queueID: UUID,
        projectID: UUID,
        segmentID: String,
        index: Int,
        totalCount: Int,
        model: MediaGenerationModel,
        size: String,
        userIdeas: String
    ) {
        self.queueID = queueID
        self.projectID = projectID
        self.segmentID = segmentID
        self.index = index
        self.totalCount = totalCount
        self.model = model
        self.size = size
        self.userIdeas = userIdeas
    }
}

/// Owns the deterministic ordering and identity checks for the user-confirmed "videos only"
/// path. Provider execution remains a separate step boundary so it can be migrated without
/// changing the established sequential continuity semantics.
public struct StoryDirectVideoQueueUseCase: Sendable {
    public static let maximumUserIdeasLength = 2_000

    public init() {}

    public func prepare(
        ownerUserID ownerInput: String,
        project: StoryProject,
        preparation: StoryMediaBatchPreparation,
        availableModels: [MediaGenerationModel],
        userIdeas: String
    ) throws -> StoryDirectVideoQueuePlan {
        let owner = ownerInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !owner.isEmpty else { throw StoryDirectVideoQueueError.invalidOwner }
        try project.validate()
        let fingerprint = try StoryProjectFingerprint.current(project)
        guard preparation.ownerUserID == owner,
              preparation.projectID == project.id,
              preparation.projectFingerprint == fingerprint,
              preparation.kind == .videos,
              preparation.executionMode == .directVideos,
              !preparation.requestedTargetIDs.isEmpty,
              Set(preparation.requestedTargetIDs).count == preparation.requestedTargetIDs.count,
              preparation.steps.map(\.targetID) == preparation.requestedTargetIDs,
              preparation.steps.allSatisfy({ $0.kind == .videos })
        else { throw StoryDirectVideoQueueError.invalidPreparation }

        let requested = Set(preparation.requestedTargetIDs)
        let orderedSegments = project.segments.filter { requested.contains($0.id) && $0.isReady }
        guard orderedSegments.count == requested.count else {
            throw StoryDirectVideoQueueError.targetUnavailable
        }
        guard let model = availableModels.first(where: {
            $0.id == project.models.videoModelID && $0.enabled && $0.hasAPIKey
        }) else { throw StoryError.missingModel }

        let profile = VideoGenerationProfile(modelName: model.modelName)
        let unsupported = orderedSegments.compactMap { segment in
            profile.durations.contains(segment.seconds)
                ? nil
                : StoryDirectVideoUnsupportedDuration(
                    segmentTitle: segment.title,
                    seconds: segment.seconds
                )
        }
        guard unsupported.isEmpty, let size = profile.sizes.first else {
            throw StoryDirectVideoQueueError.unsupportedDurations(
                supported: profile.durations,
                segments: unsupported
            )
        }

        let referenceIndexes = orderedSegments.compactMap { segment in
            switch segment.videoGuidanceMode {
            case .previousVideo, .sourceVideo:
                project.segments.firstIndex(where: { $0.id == segment.id })
            case .firstFrame, .firstAndLastFrames:
                nil
            }
        }
        guard referenceIndexes.isEmpty || model.supportsVideoReference else {
            throw StoryDirectVideoQueueError.unsupportedVideoReference
        }
        let missingPrevious = referenceIndexes.compactMap { index -> String? in
            guard project.segments[index].videoGuidanceMode == .previousVideo else { return nil }
            return index > 0 && project.segments[index - 1].video != nil
                ? nil
                : project.segments[index].title
        }
        guard missingPrevious.isEmpty else {
            throw StoryDirectVideoQueueError.missingPreviousVideos(missingPrevious)
        }
        let missingSource = referenceIndexes.compactMap { index -> String? in
            let segment = project.segments[index]
            guard segment.videoGuidanceMode == .sourceVideo else { return nil }
            return segment.archivedVideos.last == nil ? segment.title : nil
        }
        guard missingSource.isEmpty else {
            throw StoryDirectVideoQueueError.missingSourceVideos(missingSource)
        }

        return StoryDirectVideoQueuePlan(
            queueID: preparation.batchID,
            ownerUserID: owner,
            projectID: project.id,
            projectFingerprint: fingerprint,
            targetIDs: orderedSegments.map(\.id),
            model: model,
            size: size,
            userIdeas: Self.normalizedUserIdeas(userIdeas)
        )
    }

    @MainActor
    public func execute(
        _ plan: StoryDirectVideoQueuePlan,
        currentProject: @escaping @MainActor () -> StoryProject?,
        continuationCheck: @escaping @MainActor () throws -> Void,
        executeStep: @escaping @MainActor (StoryProject, StoryDirectVideoQueueStep) async throws -> Void
    ) async throws {
        try continuationCheck()
        guard let initial = currentProject(),
              initial.id == plan.projectID,
              try StoryProjectFingerprint.current(initial) == plan.projectFingerprint else {
            throw StoryDirectVideoQueueError.projectChanged
        }

        for (index, segmentID) in plan.targetIDs.enumerated() {
            try continuationCheck()
            guard let latest = currentProject(),
                  latest.id == plan.projectID,
                  latest.segments.first(where: { $0.id == segmentID })?.isReady == true else {
                throw StoryDirectVideoQueueError.targetUnavailable
            }
            let step = StoryDirectVideoQueueStep(
                queueID: plan.queueID,
                projectID: plan.projectID,
                segmentID: segmentID,
                index: index,
                totalCount: plan.targetIDs.count,
                model: plan.model,
                size: plan.size,
                userIdeas: plan.userIdeas
            )
            try await executeStep(latest, step)
            try continuationCheck()
        }
    }

    private static func normalizedUserIdeas(_ value: String) -> String {
        String(value
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .prefix(maximumUserIdeasLength))
    }
}

public struct StoryDirectVideoUnsupportedDuration: Equatable, Sendable {
    public let segmentTitle: String
    public let seconds: Int

    public init(segmentTitle: String, seconds: Int) {
        self.segmentTitle = segmentTitle
        self.seconds = seconds
    }
}

public enum StoryDirectVideoQueueError: LocalizedError, Equatable, Sendable {
    case invalidOwner
    case invalidPreparation
    case projectChanged
    case targetUnavailable
    case unsupportedDurations(
        supported: [Int],
        segments: [StoryDirectVideoUnsupportedDuration]
    )
    case unsupportedVideoReference
    case missingPreviousVideos([String])
    case missingSourceVideos([String])

    public var errorDescription: String? {
        switch self {
        case .invalidOwner:
            "剧情视频队列缺少有效的账号上下文。"
        case .invalidPreparation:
            "剧情视频队列与已确认的制作预览不一致，请重新确认。"
        case .projectChanged:
            "剧情内容已变化，请重新确认要生成的视频。"
        case .targetUnavailable:
            "所选分段已不再满足视频生成条件，请刷新后重新选择。"
        case let .unsupportedDurations(supported, segments):
            "当前视频模型不支持所选分段时长。模型支持："
                + supported.map(String.init).joined(separator: "、")
                + " 秒；不支持："
                + segments.map { "\($0.segmentTitle)（\($0.seconds)秒）" }
                    .joined(separator: "、")
        case .unsupportedVideoReference:
            "当前视频模型不支持视频参考输入。"
        case let .missingPreviousVideos(titles):
            "请先完成上一段视频，再为以下分段选择“上一段视频”："
                + titles.joined(separator: "、")
        case let .missingSourceVideos(titles):
            "找不到要重做的原视频：" + titles.joined(separator: "、")
        }
    }
}

public struct StoryDirectVideoQueueToken: Equatable, Sendable {
    public let ownerUserID: String
    public let accountSessionID: UUID
    public let projectID: UUID
    public let queueID: UUID
    public let requestID: UUID

    public init(
        ownerUserID: String,
        accountSessionID: UUID,
        projectID: UUID,
        queueID: UUID,
        requestID: UUID
    ) {
        self.ownerUserID = ownerUserID
        self.accountSessionID = accountSessionID
        self.projectID = projectID
        self.queueID = queueID
        self.requestID = requestID
    }
}

public struct StoryDirectVideoQueueState: Equatable, Sendable {
    public private(set) var activeToken: StoryDirectVideoQueueToken?

    public init() {}

    public mutating func begin(
        ownerUserID: String,
        accountSessionID: UUID,
        projectID: UUID,
        queueID: UUID
    ) -> StoryDirectVideoQueueToken? {
        guard activeToken == nil else { return nil }
        let token = StoryDirectVideoQueueToken(
            ownerUserID: ownerUserID,
            accountSessionID: accountSessionID,
            projectID: projectID,
            queueID: queueID,
            requestID: UUID()
        )
        activeToken = token
        return token
    }

    public func accepts(
        _ token: StoryDirectVideoQueueToken,
        ownerUserID: String?,
        accountSessionID: UUID,
        projectID: UUID
    ) -> Bool {
        activeToken == token
            && token.ownerUserID == ownerUserID
            && token.accountSessionID == accountSessionID
            && token.projectID == projectID
    }

    @discardableResult
    public mutating func finish(_ token: StoryDirectVideoQueueToken) -> Bool {
        guard activeToken == token else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}
