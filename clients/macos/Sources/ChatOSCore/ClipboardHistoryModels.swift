import Foundation

public enum ClipboardContentKind: String, Codable, Sendable, Hashable {
    case text
    case url
    case files
    case image
}

public struct ClipboardHistoryEntry: Identifiable, Codable, Sendable, Hashable {
    public let id: UUID
    public let kind: ClipboardContentKind
    public let createdAt: Date
    public let updatedAt: Date
    public let contentHash: String
    public let textPreview: String?
    public let sourceApplicationBundleID: String?
    public let payloadReference: String
    public let byteCount: Int64
    public let isPinned: Bool

    public init(
        id: UUID,
        kind: ClipboardContentKind,
        createdAt: Date,
        updatedAt: Date,
        contentHash: String,
        textPreview: String?,
        sourceApplicationBundleID: String?,
        payloadReference: String,
        byteCount: Int64,
        isPinned: Bool
    ) {
        self.id = id
        self.kind = kind
        self.createdAt = createdAt
        self.updatedAt = updatedAt
        self.contentHash = contentHash
        self.textPreview = textPreview
        self.sourceApplicationBundleID = sourceApplicationBundleID
        self.payloadReference = payloadReference
        self.byteCount = byteCount
        self.isPinned = isPinned
    }
}

public enum ClipboardHistoryPayload: Sendable, Hashable {
    case text(String)
    case url(URL)
    case files([URL])
    case image(data: Data, pasteboardType: String)

    public var kind: ClipboardContentKind {
        switch self {
        case .text: .text
        case .url: .url
        case .files: .files
        case .image: .image
        }
    }
}
