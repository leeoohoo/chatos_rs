import ChatOSCore
import Foundation

public struct ChatOSAttachmentService: ConversationAttachmentUploading {
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
        _ attachments: [ConversationAttachmentDraft],
        conversationID: String
    ) async throws -> [ConversationAttachmentReference] {
        guard !attachments.isEmpty else { return [] }

        let request = AttachmentUploadsRequestDTO(
            conversationID: conversationID,
            attachments: attachments.map(AttachmentUploadItemDTO.init)
        )
        let response: AttachmentUploadsResponseDTO = try await client.request(
            "/attachments/uploads",
            method: "POST",
            body: try JSONEncoder().encode(request)
        )
        guard response.uploads.count == attachments.count else {
            throw ChatOSAPIError.decoding("附件上传地址数量与文件数量不一致")
        }

        for (draft, target) in zip(attachments, response.uploads) {
            guard let url = URL(string: target.uploadURL) else {
                throw ChatOSAPIError.invalidEndpoint
            }
            var headers = (target.uploadHeaders ?? [:]).filter { key, _ in
                key.caseInsensitiveCompare("Host") != .orderedSame
                    && key.caseInsensitiveCompare("Content-Length") != .orderedSame
            }
            if !headers.keys.contains(where: { $0.caseInsensitiveCompare("Content-Type") == .orderedSame }) {
                headers["Content-Type"] = draft.mimeType
            }
            let uploadResponse = try await uploadTransport.send(
                HTTPRequest(
                    url: url,
                    method: "PUT",
                    headers: headers,
                    body: draft.data
                )
            )
            guard (200..<300).contains(uploadResponse.statusCode) else {
                throw ChatOSAPIError.server(
                    statusCode: uploadResponse.statusCode,
                    message: "附件“\(draft.name)”上传失败"
                )
            }
        }

        return response.uploads.map(\.reference)
    }
}

private struct AttachmentUploadsRequestDTO: Encodable {
    var conversationID: String
    var attachments: [AttachmentUploadItemDTO]

    enum CodingKeys: String, CodingKey {
        case attachments
        case conversationID = "conversation_id"
    }
}

private struct AttachmentUploadItemDTO: Encodable {
    var name: String
    var mimeType: String
    var size: Int
    var kind: ConversationAttachmentKind

    init(_ draft: ConversationAttachmentDraft) {
        name = draft.name
        mimeType = draft.mimeType
        size = draft.size
        kind = draft.kind
    }

    enum CodingKeys: String, CodingKey {
        case name, mimeType, size
        case kind = "type"
    }
}

private struct AttachmentUploadsResponseDTO: Decodable, Sendable {
    var uploads: [AttachmentUploadTargetDTO]
}

private struct AttachmentUploadTargetDTO: Decodable, Sendable {
    var id: String?
    var name: String
    var mimeType: String
    var size: Int
    var kind: ConversationAttachmentKind
    var storageProvider: String?
    var bucket: String?
    var objectKey: String?
    var uploadURL: String
    var uploadHeaders: [String: String]?
    var url: String?
    var viewURL: String?

    enum CodingKeys: String, CodingKey {
        case id, name, mimeType, size, bucket, url
        case kind = "type"
        case storageProvider, objectKey, uploadHeaders
        case uploadURL = "uploadUrl"
        case viewURL = "viewUrl"
    }

    var reference: ConversationAttachmentReference {
        ConversationAttachmentReference(
            id: id ?? UUID().uuidString.lowercased(),
            name: name,
            mimeType: mimeType,
            size: size,
            kind: kind,
            storageProvider: storageProvider,
            bucket: bucket,
            objectKey: objectKey,
            url: url,
            viewURL: viewURL
        )
    }
}
