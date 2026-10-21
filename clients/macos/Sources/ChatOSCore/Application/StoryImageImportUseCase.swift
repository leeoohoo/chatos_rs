import Foundation

public enum StoryImageImportTarget: Equatable, Sendable {
    case resource(id: String)
    case frame(segmentID: String, role: StoryFrameRole)
}

public enum StoryImageImportSource: Equatable, Sendable {
    case generatedAsset
    case localFile
}

public struct StoryImageImportCommand: Equatable, Sendable {
    public let ownerUserID: String
    public let project: StoryProject
    public let target: StoryImageImportTarget
    public let preservingVideoSegmentIDs: Set<String>
    public let confirmsImportedImage: Bool

    public init(
        ownerUserID: String,
        project: StoryProject,
        target: StoryImageImportTarget,
        preservingVideoSegmentIDs: Set<String>,
        confirmsImportedImage: Bool
    ) {
        self.ownerUserID = ownerUserID
        self.project = project
        self.target = target
        self.preservingVideoSegmentIDs = preservingVideoSegmentIDs
        self.confirmsImportedImage = confirmsImportedImage
    }
}

public protocol StoryImageImporting: Sendable {
    func importGeneratedStoryImage(
        command: StoryImageImportCommand,
        asset: GeneratedMediaAsset
    ) async throws -> StoryProject

    func importLocalStoryImage(
        command: StoryImageImportCommand,
        fileURL: URL
    ) async throws -> StoryProject
}

/// Application boundary for attaching an existing generated image or a user-selected
/// local image to one Story resource/frame. Reading, decoding, normalization and disk
/// persistence remain entirely behind the platform adapter.
public struct StoryImageImportUseCase: Sendable {
    private let importer: any StoryImageImporting

    public init(importer: any StoryImageImporting) {
        self.importer = importer
    }

    public func prepare(
        ownerUserID ownerInput: String,
        project: StoryProject,
        target: StoryImageImportTarget,
        preservingVideoSegmentIDs: Set<String>,
        confirmsImportedImage: Bool = true
    ) throws -> StoryImageImportCommand {
        let owner = ownerInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !owner.isEmpty else { throw StoryImageImportError.invalidOwner }
        try project.validate()
        try validate(target: target, in: project)
        return .init(
            ownerUserID: owner,
            project: project,
            target: target,
            preservingVideoSegmentIDs: preservingVideoSegmentIDs,
            confirmsImportedImage: confirmsImportedImage
        )
    }

    public func importGeneratedAsset(
        _ asset: GeneratedMediaAsset,
        command: StoryImageImportCommand
    ) async throws -> StoryProject {
        let result = try await importer.importGeneratedStoryImage(
            command: command,
            asset: asset
        )
        return try validated(result, for: command)
    }

    public func importLocalFile(
        _ fileURL: URL,
        command: StoryImageImportCommand
    ) async throws -> StoryProject {
        let result = try await importer.importLocalStoryImage(
            command: command,
            fileURL: fileURL
        )
        return try validated(result, for: command)
    }

    private func validate(
        target: StoryImageImportTarget,
        in project: StoryProject
    ) throws {
        switch target {
        case .resource(let id):
            guard project.resource(id: id) != nil else {
                throw StoryImageImportError.invalidTarget
            }
        case .frame(let segmentID, _):
            guard let segment = project.segments.first(where: { $0.id == segmentID }),
                  segment.attempt == nil else {
                throw StoryImageImportError.invalidTarget
            }
        }
    }

    private func validated(
        _ result: StoryProject,
        for command: StoryImageImportCommand
    ) throws -> StoryProject {
        try Task.checkCancellation()
        guard result.id == command.project.id else {
            throw StoryImageImportError.resultMismatch
        }
        try result.validate()
        let before: [StoryImage]
        let after: [StoryImage]
        switch command.target {
        case .resource(let id):
            guard let old = command.project.resource(id: id),
                  let new = result.resource(id: id) else {
                throw StoryImageImportError.resultMismatch
            }
            before = old.images
            after = new.images
        case .frame(let segmentID, let role):
            guard let old = command.project.segments.first(where: { $0.id == segmentID }),
                  let new = result.segments.first(where: { $0.id == segmentID }) else {
                throw StoryImageImportError.resultMismatch
            }
            switch role {
            case .first:
                before = old.firstFrames.images
                after = new.firstFrames.images
            case .last:
                before = old.lastFrames.images
                after = new.lastFrames.images
            }
        }
        guard after.count == before.count + 1,
              Array(after.dropLast()) == before else {
            throw StoryImageImportError.resultMismatch
        }
        return result
    }
}

public enum StoryImageImportError: LocalizedError, Equatable, Sendable {
    case invalidOwner
    case invalidTarget
    case resultMismatch

    public var errorDescription: String? {
        switch self {
        case .invalidOwner:
            "图片导入缺少有效的账号上下文。"
        case .invalidTarget:
            "图片导入目标已变化，请重新选择后再试。"
        case .resultMismatch:
            "图片导入结果与当前剧情不匹配，已拒绝写入界面。"
        }
    }
}

public struct StoryImageImportToken: Equatable, Sendable {
    public let ownerUserID: String
    public let accountSessionID: UUID
    public let projectID: UUID
    public let target: StoryImageImportTarget
    public let source: StoryImageImportSource
    public let requestID: UUID
}

public struct StoryImageImportState: Equatable, Sendable {
    public private(set) var activeToken: StoryImageImportToken?

    public init() {}

    public mutating func begin(
        command: StoryImageImportCommand,
        accountSessionID: UUID,
        source: StoryImageImportSource
    ) -> StoryImageImportToken? {
        guard activeToken == nil else { return nil }
        let token = StoryImageImportToken(
            ownerUserID: command.ownerUserID,
            accountSessionID: accountSessionID,
            projectID: command.project.id,
            target: command.target,
            source: source,
            requestID: UUID()
        )
        activeToken = token
        return token
    }

    public func accepts(
        _ token: StoryImageImportToken,
        ownerUserID: String?,
        accountSessionID: UUID,
        projectID: UUID?
    ) -> Bool {
        activeToken == token
            && token.ownerUserID == ownerUserID
            && token.accountSessionID == accountSessionID
            && token.projectID == projectID
    }

    @discardableResult
    public mutating func finish(_ token: StoryImageImportToken) -> Bool {
        guard activeToken == token else { return false }
        activeToken = nil
        return true
    }

    public mutating func reset() {
        activeToken = nil
    }
}
