import Foundation

public enum ConversationAttachmentKind: String, Codable, Sendable, Equatable {
    case image
    case file
    case audio
}

public enum ConversationAttachmentOrigin: String, Codable, Sendable, Equatable {
    case file
    case pastedImage
    case pastedDocument
    case pastedText
}

public struct ConversationAttachmentDraft: Identifiable, Sendable, Equatable {
    public var id: String
    public var name: String
    public var mimeType: String
    public var kind: ConversationAttachmentKind
    public var origin: ConversationAttachmentOrigin
    public var data: Data

    public init(
        id: String = UUID().uuidString.lowercased(),
        name: String,
        mimeType: String,
        kind: ConversationAttachmentKind,
        origin: ConversationAttachmentOrigin,
        data: Data
    ) {
        self.id = id
        self.name = name
        self.mimeType = mimeType
        self.kind = kind
        self.origin = origin
        self.data = data
    }

    public var size: Int { data.count }

    public var reference: ConversationAttachmentReference {
        ConversationAttachmentReference(
            id: id,
            name: name,
            mimeType: mimeType,
            size: size,
            kind: kind
        )
    }
}

public struct ConversationAttachmentReference: Identifiable, Codable, Sendable, Equatable {
    public var id: String
    public var name: String
    public var mimeType: String
    public var size: Int
    public var kind: ConversationAttachmentKind
    public var storageProvider: String?
    public var bucket: String?
    public var objectKey: String?
    public var url: String?
    public var viewURL: String?

    public init(
        id: String = UUID().uuidString.lowercased(),
        name: String,
        mimeType: String,
        size: Int,
        kind: ConversationAttachmentKind,
        storageProvider: String? = nil,
        bucket: String? = nil,
        objectKey: String? = nil,
        url: String? = nil,
        viewURL: String? = nil
    ) {
        self.id = id
        self.name = name
        self.mimeType = mimeType
        self.size = size
        self.kind = kind
        self.storageProvider = storageProvider
        self.bucket = bucket
        self.objectKey = objectKey
        self.url = url
        self.viewURL = viewURL
    }

    enum CodingKeys: String, CodingKey {
        case id, name, size, bucket, url
        case mimeType
        case kind = "type"
        case storageProvider
        case objectKey
        case viewURL = "viewUrl"
    }
}

public protocol ConversationAttachmentUploading: Sendable {
    func upload(
        _ attachments: [ConversationAttachmentDraft],
        conversationID: String
    ) async throws -> [ConversationAttachmentReference]
}
