import Foundation

public protocol StoryMediaAccessResolving: Sendable {
    func resolveStoryMediaURL(
        ownerUserID: String,
        projectID: UUID,
        filename: String
    ) throws -> URL
}

/// Read-only projection used by native presentation code. The UI receives a displayable
/// media reference but never knows how an owner/project filename maps onto local storage.
public struct StoryMediaAccessUseCase: Sendable {
    private let resolver: any StoryMediaAccessResolving

    public init(resolver: any StoryMediaAccessResolving) {
        self.resolver = resolver
    }

    public func imageAsset(
        ownerUserID ownerInput: String,
        projectID: UUID,
        image: StoryImage
    ) throws -> GeneratedMediaAsset {
        guard image.mimeType.lowercased().hasPrefix("image/") else {
            throw StoryMediaAccessError.invalidMedia
        }
        let url = try resolve(
            ownerUserID: ownerInput,
            projectID: projectID,
            filename: image.filename
        )
        return .init(
            id: image.id.uuidString,
            mimeType: image.mimeType,
            url: url
        )
    }

    public func videoURL(
        ownerUserID ownerInput: String,
        projectID: UUID,
        video: StoryVideo
    ) throws -> URL {
        guard !video.jobID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw StoryMediaAccessError.invalidMedia
        }
        return try resolve(
            ownerUserID: ownerInput,
            projectID: projectID,
            filename: video.filename
        )
    }

    private func resolve(
        ownerUserID ownerInput: String,
        projectID: UUID,
        filename: String
    ) throws -> URL {
        let owner = ownerInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !owner.isEmpty else { throw StoryMediaAccessError.invalidOwner }
        guard !filename.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw StoryMediaAccessError.invalidMedia
        }
        let url = try resolver.resolveStoryMediaURL(
            ownerUserID: owner,
            projectID: projectID,
            filename: filename
        )
        guard url.isFileURL else { throw StoryMediaAccessError.invalidResolution }
        return url
    }
}

public enum StoryMediaAccessError: LocalizedError, Equatable, Sendable {
    case invalidOwner
    case invalidMedia
    case invalidResolution

    public var errorDescription: String? {
        switch self {
        case .invalidOwner:
            "剧情媒体缺少有效的账号上下文。"
        case .invalidMedia:
            "剧情媒体记录无效。"
        case .invalidResolution:
            "剧情媒体地址不属于本机安全存储。"
        }
    }
}
