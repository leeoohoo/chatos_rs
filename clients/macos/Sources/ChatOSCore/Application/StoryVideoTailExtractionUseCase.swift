import Foundation

public struct StoryVideoTailExtractionCommand: Equatable, Sendable {
    public let ownerUserID: String
    public let projectID: UUID
    public let segmentID: String
    public let videoJobID: String
    public let videoFilename: String

    public init(
        ownerUserID: String,
        projectID: UUID,
        segmentID: String,
        videoJobID: String,
        videoFilename: String
    ) {
        self.ownerUserID = ownerUserID
        self.projectID = projectID
        self.segmentID = segmentID
        self.videoJobID = videoJobID
        self.videoFilename = videoFilename
    }
}

public protocol StoryVideoTailExtracting: Sendable {
    func extractStoryVideoTail(
        command: StoryVideoTailExtractionCommand
    ) async throws -> StoryProject
}

/// Application boundary for explicitly rebuilding the actual tail frame of a completed
/// local video. File resolution, media decoding and persistence belong to the adapter.
public struct StoryVideoTailExtractionUseCase: Sendable {
    private let extractor: any StoryVideoTailExtracting

    public init(extractor: any StoryVideoTailExtracting) {
        self.extractor = extractor
    }

    public func prepare(
        ownerUserID ownerInput: String,
        project: StoryProject,
        segmentID: String
    ) throws -> StoryVideoTailExtractionCommand {
        let owner = ownerInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !owner.isEmpty else { throw StoryVideoTailExtractionError.invalidOwner }
        guard let segment = project.segments.first(where: { $0.id == segmentID }),
              let video = segment.video,
              !video.jobID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              !video.filename.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw StoryVideoTailExtractionError.invalidTarget
        }
        return .init(
            ownerUserID: owner,
            projectID: project.id,
            segmentID: segmentID,
            videoJobID: video.jobID,
            videoFilename: video.filename
        )
    }

    public func extract(
        _ command: StoryVideoTailExtractionCommand
    ) async throws -> StoryProject {
        let project = try await extractor.extractStoryVideoTail(command: command)
        try Task.checkCancellation()
        guard project.id == command.projectID,
              let segment = project.segments.first(where: { $0.id == command.segmentID }),
              segment.video?.jobID == command.videoJobID,
              let tail = segment.actualVideoLastFrame,
              tail.derivedFromVideoJobID == command.videoJobID else {
            throw StoryVideoTailExtractionError.resultMismatch
        }
        try project.validate()
        return project
    }
}

public enum StoryVideoTailExtractionError: LocalizedError, Equatable, Sendable {
    case invalidOwner
    case invalidTarget
    case resultMismatch

    public var errorDescription: String? {
        switch self {
        case .invalidOwner:
            "视频末帧提取缺少有效的账号上下文。"
        case .invalidTarget:
            "当前分段没有可提取末帧的完成视频。"
        case .resultMismatch:
            "视频末帧结果与当前任务不匹配，已拒绝写入界面。"
        }
    }
}

public struct StoryVideoTailExtractionTarget: Hashable, Sendable {
    public let projectID: UUID
    public let segmentID: String
}

public struct StoryVideoTailExtractionToken: Equatable, Sendable {
    public let ownerUserID: String
    public let accountSessionID: UUID
    public let target: StoryVideoTailExtractionTarget
    public let videoJobID: String
    public let requestID: UUID
}

public struct StoryVideoTailExtractionState: Equatable, Sendable {
    public private(set) var activeTokens: [
        StoryVideoTailExtractionTarget: StoryVideoTailExtractionToken
    ] = [:]

    public init() {}

    public mutating func begin(
        command: StoryVideoTailExtractionCommand,
        accountSessionID: UUID
    ) -> StoryVideoTailExtractionToken? {
        let target = StoryVideoTailExtractionTarget(
            projectID: command.projectID,
            segmentID: command.segmentID
        )
        guard activeTokens[target] == nil else { return nil }
        let token = StoryVideoTailExtractionToken(
            ownerUserID: command.ownerUserID,
            accountSessionID: accountSessionID,
            target: target,
            videoJobID: command.videoJobID,
            requestID: UUID()
        )
        activeTokens[target] = token
        return token
    }

    public func accepts(
        _ token: StoryVideoTailExtractionToken,
        ownerUserID: String?,
        accountSessionID: UUID
    ) -> Bool {
        activeTokens[token.target] == token
            && token.ownerUserID == ownerUserID
            && token.accountSessionID == accountSessionID
    }

    @discardableResult
    public mutating func finish(_ token: StoryVideoTailExtractionToken) -> Bool {
        guard activeTokens[token.target] == token else { return false }
        activeTokens[token.target] = nil
        return true
    }

    public mutating func reset() {
        activeTokens = [:]
    }
}
