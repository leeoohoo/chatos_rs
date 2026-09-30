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
    public var sha256: String?
    public var localURL: URL?

    public init(
        id: String = UUID().uuidString.lowercased(),
        name: String,
        mimeType: String,
        size: Int,
        kind: ConversationAttachmentKind,
        sha256: String? = nil,
        localURL: URL? = nil
    ) {
        self.id = id
        self.name = name
        self.mimeType = mimeType
        self.size = size
        self.kind = kind
        self.sha256 = sha256
        self.localURL = localURL
    }

    enum CodingKeys: String, CodingKey {
        case id, name, size, sha256, localURL
        case mimeType
        case kind = "type"
    }
}
