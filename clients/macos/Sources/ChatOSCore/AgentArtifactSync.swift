import Foundation

public enum ProjectAgentMessageAttachmentSyncStatus: String, Codable, Sendable {
    case localOnly = "local_only"
    case queued
    case uploading
    case synced
    case failed
}

public struct AgentArtifactUploadRequest: Sendable, Equatable {
    public let name: String
    public let mimeType: String
    public let data: Data
    public let sha256: String
    public let idempotencyKey: String

    public init(
        name: String,
        mimeType: String,
        data: Data,
        sha256: String,
        idempotencyKey: String
    ) {
        self.name = name
        self.mimeType = mimeType
        self.data = data
        self.sha256 = sha256
        self.idempotencyKey = idempotencyKey
    }
}

public struct AgentArtifactRemoteMetadata: Sendable, Equatable {
    public let artifactID: String
    public let name: String
    public let mimeType: String
    public let size: Int
    public let sha256: String
    public let storageProvider: String?
    public let bucket: String?
    public let objectKey: String?
    public let remoteViewPath: String?

    public init(
        artifactID: String,
        name: String,
        mimeType: String,
        size: Int,
        sha256: String,
        storageProvider: String? = nil,
        bucket: String? = nil,
        objectKey: String? = nil,
        remoteViewPath: String? = nil
    ) {
        self.artifactID = artifactID
        self.name = name
        self.mimeType = mimeType
        self.size = size
        self.sha256 = sha256
        self.storageProvider = storageProvider
        self.bucket = bucket
        self.objectKey = objectKey
        self.remoteViewPath = remoteViewPath
    }
}

/// Authenticated account service for Agent-authored Markdown artifacts. Implementations must not
/// expose upload URLs, object keys or authorization material to an Agent tool result.
public protocol AgentArtifactRemoteServing: Sendable {
    func upload(_ request: AgentArtifactUploadRequest) async throws -> AgentArtifactRemoteMetadata
    func download(artifactID: String) async throws -> Data
    func delete(artifactID: String) async throws
}
