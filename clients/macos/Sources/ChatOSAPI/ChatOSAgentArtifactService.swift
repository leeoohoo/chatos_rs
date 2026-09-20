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
              Self.isArtifactID(target.artifactID),
              target.size == request.data.count,
              target.sha256 == request.sha256,
              Self.isUTF8Markdown(target.mimeType) else {
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
              completed.status == "uploaded",
              Self.isUTF8Markdown(completed.mimeType) else {
            throw ChatOSAPIError.invalidResponse
        }
        return target.metadata
    }

    public func download(artifactID: String) async throws -> Data {
        guard Self.isArtifactID(artifactID) else { throw ChatOSAPIError.invalidResponse }
        try Task.checkCancellation()
        let authenticationSessionID = try await client.currentAuthenticationSessionID()
        let response = try await client.requestData(
            "/agent-artifacts/\(artifactID)/content",
            additionalHeaders: ["Accept": "text/markdown"],
            expectedAuthenticationSessionID: authenticationSessionID
        )
        guard !response.body.isEmpty,
              response.body.count <= AgentCommunicationPolicy.standard.maximumDocumentBytes,
              String(data: response.body, encoding: .utf8) != nil,
              Self.isUTF8Markdown(response.headers.first(where: {
                  $0.key.caseInsensitiveCompare("Content-Type") == .orderedSame
              })?.value) else {
            throw ChatOSAPIError.invalidResponse
        }
        return response.body
    }

    public func list(limit: Int, cursor: String?) async throws -> AgentArtifactRemotePage {
        guard (1...100).contains(limit) else {
            throw ChatOSAPIError.invalidRequest("Agent artifact page limit must be 1...100")
        }
        try Task.checkCancellation()
        let authenticationSessionID = try await client.currentAuthenticationSessionID()
        var components = URLComponents()
        components.path = "/agent-artifacts"
        components.queryItems = [URLQueryItem(name: "limit", value: String(limit))]
        if let cursor, !cursor.isEmpty {
            components.queryItems?.append(URLQueryItem(name: "cursor", value: cursor))
        }
        guard let endpoint = components.string else { throw ChatOSAPIError.invalidEndpoint }
        let response: AgentArtifactListResponseDTO = try await client.request(
            endpoint,
            expectedAuthenticationSessionID: authenticationSessionID
        )
        let artifacts = try response.artifacts.map { item in
            guard item.status == "uploaded",
                  Self.isArtifactID(item.artifactID),
                  item.size > 0,
                  item.size <= AgentCommunicationPolicy.standard.maximumDocumentBytes,
                  item.sha256.count == 64,
                  Self.isUTF8Markdown(item.mimeType),
                  let createdAtUnixMs = Self.unixMilliseconds(item.createdAt),
                  let updatedAtUnixMs = Self.unixMilliseconds(item.updatedAt) else {
                throw ChatOSAPIError.invalidResponse
            }
            return AgentArtifactRemoteItem(
                artifactID: item.artifactID,
                name: item.name,
                mimeType: item.mimeType,
                size: item.size,
                sha256: item.sha256,
                status: item.status,
                remoteViewPath: item.remoteViewPath,
                createdAtUnixMs: createdAtUnixMs,
                updatedAtUnixMs: updatedAtUnixMs
            )
        }
        return .init(artifacts: artifacts, nextCursor: response.nextCursor)
    }

    public func delete(artifactID: String) async throws {
        guard Self.isArtifactID(artifactID) else { throw ChatOSAPIError.invalidResponse }
        try Task.checkCancellation()
        let authenticationSessionID = try await client.currentAuthenticationSessionID()
        _ = try await client.requestData(
            "/agent-artifacts/\(artifactID)",
            method: "DELETE",
            expectedAuthenticationSessionID: authenticationSessionID
        )
    }

    private static func isUTF8Markdown(_ value: String?) -> Bool {
        guard let value else { return false }
        let components = value.split(separator: ";", omittingEmptySubsequences: true)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() }
        guard components.first == "text/markdown" else { return false }
        let charsetValues = components.dropFirst().compactMap { component -> String? in
            guard component.hasPrefix("charset=") else { return nil }
            return String(component.dropFirst("charset=".count))
                .trimmingCharacters(in: CharacterSet(charactersIn: "\"'"))
        }
        return charsetValues.isEmpty || charsetValues.allSatisfy { $0 == "utf-8" }
    }

    private static func isArtifactID(_ value: String) -> Bool {
        value.count == 41
            && value.hasPrefix("artifact_")
            && value.dropFirst("artifact_".count).allSatisfy(\.isHexDigit)
    }

    private static func unixMilliseconds(_ value: String) -> Int64? {
        let fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let wholeSeconds = ISO8601DateFormatter()
        wholeSeconds.formatOptions = [.withInternetDateTime]
        guard let date = fractional.date(from: value) ?? wholeSeconds.date(from: value) else {
            return nil
        }
        return Int64(date.timeIntervalSince1970 * 1_000)
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

private struct AgentArtifactListResponseDTO: Decodable, Sendable {
    let artifacts: [AgentArtifactListItemDTO]
    let nextCursor: String?
}

private struct AgentArtifactListItemDTO: Decodable, Sendable {
    let artifactID: String
    let name: String
    let mimeType: String
    let size: Int
    let sha256: String
    let status: String
    let remoteViewPath: String?
    let createdAt: String
    let updatedAt: String

    private enum CodingKeys: String, CodingKey {
        case name, mimeType, size, sha256, status, remoteViewPath, createdAt, updatedAt
        case artifactID = "artifactId"
    }
}
