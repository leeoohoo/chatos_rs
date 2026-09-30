import Foundation

public enum ProjectAgentMessageAttachmentSyncStatus: String, Codable, Sendable {
    case localOnly = "local_only"
    case queued
    case uploading
    case synced
    case failed
}

public struct AgentArtifactWriteRequest: Sendable, Equatable {
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

public struct AgentArtifactMetadata: Sendable, Equatable {
    public let artifactID: String
    public let name: String
    public let mimeType: String
    public let size: Int
    public let sha256: String

    public init(
        artifactID: String,
        name: String,
        mimeType: String,
        size: Int,
        sha256: String
    ) {
        self.artifactID = artifactID
        self.name = name
        self.mimeType = mimeType
        self.size = size
        self.sha256 = sha256
    }
}

public struct AgentArtifactItem: Identifiable, Sendable, Equatable {
    public let artifactID: String
    public let name: String
    public let mimeType: String
    public let size: Int
    public let sha256: String
    public let status: String
    public let createdAtUnixMs: Int64
    public let updatedAtUnixMs: Int64

    public var id: String { artifactID }

    public init(
        artifactID: String,
        name: String,
        mimeType: String,
        size: Int,
        sha256: String,
        status: String,
        createdAtUnixMs: Int64,
        updatedAtUnixMs: Int64
    ) {
        self.artifactID = artifactID
        self.name = name
        self.mimeType = mimeType
        self.size = size
        self.sha256 = sha256
        self.status = status
        self.createdAtUnixMs = createdAtUnixMs
        self.updatedAtUnixMs = updatedAtUnixMs
    }
}

public struct AgentArtifactPage: Sendable, Equatable {
    public let artifacts: [AgentArtifactItem]
    public let nextCursor: String?

    public init(artifacts: [AgentArtifactItem], nextCursor: String?) {
        self.artifacts = artifacts
        self.nextCursor = nextCursor
    }
}

/// Account-scoped local persistence for Agent-authored Markdown artifacts.
public protocol AgentArtifactServing: Sendable {
    func store(
        ownerUserID: String,
        request: AgentArtifactWriteRequest
    ) async throws -> AgentArtifactMetadata
    func list(ownerUserID: String, limit: Int, cursor: String?) async throws -> AgentArtifactPage
    func read(ownerUserID: String, artifactID: String) async throws -> Data
    func remove(ownerUserID: String, artifactID: String) async throws
}
