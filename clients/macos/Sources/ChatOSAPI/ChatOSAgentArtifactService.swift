import ChatOSCore
import Foundation

public struct ChatOSAgentArtifactService: AgentArtifactRemoteServing {
    private let client: ChatOSAPIClient
    private let uploadTransport: any HTTPTransport

    public init(
        client: ChatOSAPIClient,
        uploadTransport: any HTTPTransport = URLSessionHTTPTransport()
    ) {
        self.client = client
        self.uploadTransport = uploadTransport
    }

    public func upload(
        _ request: AgentArtifactUploadRequest
    ) async throws -> AgentArtifactRemoteMetadata {
        try Task.checkCancellation()
        let authenticationSessionID = try await client.currentAuthenticationSessionID()
        let payload = CreateAgentArtifactUploadsRequestDTO(artifacts: [
            .init(
                name: request.name,
                mimeType: request.mimeType,
                size: request.data.count,
                sha256: request.sha256,
                idempotencyKey: request.idempotencyKey
            ),
        ])
        let response: AgentArtifactUploadsResponseDTO = try await client.request(
            "/agent-artifacts/uploads",
            method: "POST",
            body: try JSONEncoder().encode(payload),
            expectedAuthenticationSessionID: authenticationSessionID
        )
        guard response.uploads.count == 1, let target = response.uploads.first,
              target.size == request.data.count,
              target.sha256 == request.sha256 else {
            throw ChatOSAPIError.invalidResponse
        }
        guard let uploadURL = URL(string: target.uploadURL) else {
            throw ChatOSAPIError.invalidEndpoint
        }
        var headers = (target.uploadHeaders ?? [:]).filter { key, _ in
            key.caseInsensitiveCompare("Host") != .orderedSame
                && key.caseInsensitiveCompare("Content-Length") != .orderedSame
        }
        if !headers.keys.contains(where: {
            $0.caseInsensitiveCompare("Content-Type") == .orderedSame
        }) {
            headers["Content-Type"] = request.mimeType
        }
        let uploadResponse = try await uploadTransport.send(HTTPRequest(
            url: uploadURL,
            method: "PUT",
            headers: headers,
            body: request.data
        ))
        guard (200..<300).contains(uploadResponse.statusCode) else {
            throw ChatOSAPIError.server(
                statusCode: uploadResponse.statusCode,
                message: "Agent 文档上传失败"
            )
        }
        try Task.checkCancellation()
        let completed: AgentArtifactMetadataDTO = try await client.request(
            "/agent-artifacts/\(target.artifactID)/complete",
            method: "POST",
            expectedAuthenticationSessionID: authenticationSessionID
        )
        guard completed.artifactID == target.artifactID,
              completed.size == request.data.count,
              completed.sha256 == request.sha256,
              completed.status == "uploaded" else {
            throw ChatOSAPIError.invalidResponse
        }
        return target.metadata
    }

    public func download(artifactID: String) async throws -> Data {
        try Task.checkCancellation()
        let authenticationSessionID = try await client.currentAuthenticationSessionID()
        let response = try await client.requestData(
            "/agent-artifacts/\(artifactID)/content",
            additionalHeaders: ["Accept": "text/markdown"],
            expectedAuthenticationSessionID: authenticationSessionID
        )
        guard !response.body.isEmpty,
              response.body.count <= AgentCommunicationPolicy.standard.maximumDocumentBytes else {
            throw ChatOSAPIError.invalidResponse
        }
        return response.body
    }

    public func delete(artifactID: String) async throws {
        try Task.checkCancellation()
        let authenticationSessionID = try await client.currentAuthenticationSessionID()
        _ = try await client.requestData(
            "/agent-artifacts/\(artifactID)",
            method: "DELETE",
            expectedAuthenticationSessionID: authenticationSessionID
        )
    }
}

private struct CreateAgentArtifactUploadsRequestDTO: Encodable {
    let artifacts: [CreateAgentArtifactUploadItemDTO]
}

private struct CreateAgentArtifactUploadItemDTO: Encodable {
    let name: String
    let mimeType: String
    let size: Int
    let sha256: String
    let idempotencyKey: String
}

private struct AgentArtifactUploadsResponseDTO: Decodable, Sendable {
    let uploads: [AgentArtifactUploadTargetDTO]
}

private struct AgentArtifactUploadTargetDTO: Decodable, Sendable {
    let artifactID: String
    let name: String
    let mimeType: String
    let size: Int
    let sha256: String
    let status: String
    let storageProvider: String?
    let bucket: String?
    let objectKey: String?
    let uploadURL: String
    let uploadHeaders: [String: String]?
    let remoteViewPath: String?

    private enum CodingKeys: String, CodingKey {
        case name, mimeType, size, sha256, status, storageProvider, bucket, objectKey
        case uploadHeaders, remoteViewPath
        case artifactID = "artifactId"
        case uploadURL = "uploadUrl"
    }

    var metadata: AgentArtifactRemoteMetadata {
        .init(
            artifactID: artifactID,
            name: name,
            mimeType: mimeType,
            size: size,
            sha256: sha256,
            storageProvider: storageProvider,
            bucket: bucket,
            objectKey: objectKey,
            remoteViewPath: remoteViewPath
        )
    }
}

private struct AgentArtifactMetadataDTO: Decodable, Sendable {
    let artifactID: String
    let name: String
    let mimeType: String
    let size: Int
    let sha256: String
    let status: String
    let remoteViewPath: String?

    private enum CodingKeys: String, CodingKey {
        case name, mimeType, size, sha256, status, remoteViewPath
        case artifactID = "artifactId"
    }
}
