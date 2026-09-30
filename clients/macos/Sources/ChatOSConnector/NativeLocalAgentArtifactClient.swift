import ChatOSCore
import Foundation

public struct NativeLocalAgentArtifactClient: AgentArtifactServing, Sendable {
    private let host: any LocalAgentHostClientServicing

    public init(host: any LocalAgentHostClientServicing) {
        self.host = host
    }

    public func store(
        ownerUserID: String,
        request: AgentArtifactWriteRequest
    ) async throws -> AgentArtifactMetadata {
        let response: ArtifactResult = try await host.request(CreateCommand(
            type: "create_artifact",
            ownerUserID: ownerUserID,
            name: request.name,
            mimeType: request.mimeType,
            dataBase64: request.data.base64EncodedString(),
            sha256: request.sha256,
            idempotencyKey: request.idempotencyKey
        ))
        guard response.type == "artifact" else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return response.artifact.metadata
    }

    public func list(
        ownerUserID: String,
        limit: Int,
        cursor: String?
    ) async throws -> AgentArtifactPage {
        let response: ArtifactPageResult = try await host.request(ListCommand(
            type: "list_artifacts",
            ownerUserID: ownerUserID,
            limit: UInt32(clamping: limit),
            cursor: cursor
        ))
        guard response.type == "artifacts" else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return .init(
            artifacts: response.page.artifacts.map(\.item),
            nextCursor: response.page.nextCursor
        )
    }

    public func read(ownerUserID: String, artifactID: String) async throws -> Data {
        let response: ArtifactDataResult = try await host.request(IdentityCommand(
            type: "get_artifact_data",
            ownerUserID: ownerUserID,
            artifactID: artifactID
        ))
        guard response.type == "artifact_data",
              response.artifactID == artifactID,
              let data = Data(base64Encoded: response.dataBase64) else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return data
    }

    public func remove(ownerUserID: String, artifactID: String) async throws {
        let response: ArtifactDeletedResult = try await host.request(IdentityCommand(
            type: "delete_artifact",
            ownerUserID: ownerUserID,
            artifactID: artifactID
        ))
        guard response.type == "artifact_deleted", response.artifactID == artifactID else {
            throw NativeLocalAgentHostError.invalidResponse
        }
    }
}

private struct CreateCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let name: String
    let mimeType: String
    let dataBase64: String
    let sha256: String
    let idempotencyKey: String

    enum CodingKeys: String, CodingKey {
        case type, name, sha256
        case ownerUserID = "owner_user_id"
        case mimeType = "mime_type"
        case dataBase64 = "data_base64"
        case idempotencyKey = "idempotency_key"
    }
}

private struct ListCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let limit: UInt32
    let cursor: String?

    enum CodingKeys: String, CodingKey {
        case type, limit, cursor
        case ownerUserID = "owner_user_id"
    }
}

private struct IdentityCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let artifactID: String

    enum CodingKeys: String, CodingKey {
        case type
        case ownerUserID = "owner_user_id"
        case artifactID = "artifact_id"
    }
}

private struct ArtifactResult: Decodable, Sendable {
    let type: String
    let artifact: ArtifactRecord
}

private struct ArtifactPageResult: Decodable, Sendable {
    let type: String
    let page: ArtifactPageRecord
}

private struct ArtifactPageRecord: Decodable, Sendable {
    let artifacts: [ArtifactRecord]
    let nextCursor: String?

    enum CodingKeys: String, CodingKey {
        case artifacts
        case nextCursor = "next_cursor"
    }
}

private struct ArtifactDataResult: Decodable, Sendable {
    let type: String
    let artifactID: String
    let dataBase64: String

    enum CodingKeys: String, CodingKey {
        case type
        case artifactID = "artifact_id"
        case dataBase64 = "data_base64"
    }
}

private struct ArtifactDeletedResult: Decodable, Sendable {
    let type: String
    let artifactID: String

    enum CodingKeys: String, CodingKey {
        case type
        case artifactID = "artifact_id"
    }
}

private struct ArtifactRecord: Decodable, Sendable {
    let artifactID: String
    let name: String
    let mimeType: String
    let size: UInt64
    let sha256: String
    let createdAtUnixMs: Int64
    let updatedAtUnixMs: Int64

    enum CodingKeys: String, CodingKey {
        case name, size, sha256
        case artifactID = "artifact_id"
        case mimeType = "mime_type"
        case createdAtUnixMs = "created_at_unix_ms"
        case updatedAtUnixMs = "updated_at_unix_ms"
    }

    var metadata: AgentArtifactMetadata {
        .init(
            artifactID: artifactID,
            name: name,
            mimeType: mimeType,
            size: Int(clamping: size),
            sha256: sha256
        )
    }

    var item: AgentArtifactItem {
        .init(
            artifactID: artifactID,
            name: name,
            mimeType: mimeType,
            size: Int(clamping: size),
            sha256: sha256,
            status: "stored",
            createdAtUnixMs: createdAtUnixMs,
            updatedAtUnixMs: updatedAtUnixMs
        )
    }
}
