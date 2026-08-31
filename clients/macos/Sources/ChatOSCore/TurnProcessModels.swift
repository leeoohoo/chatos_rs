import Foundation

public struct TurnProcessNode: Sendable, Equatable, Identifiable {
    public enum Kind: String, Sendable, Equatable {
        case task
        case tool
        case reasoning
        case update
    }

    public let id: String
    public var title: String
    public var detail: String?
    public var status: TurnStatus
    public var kind: Kind
    public var timestamp: Date?

    public init(
        id: String,
        title: String,
        detail: String?,
        status: TurnStatus,
        kind: Kind,
        timestamp: Date?
    ) {
        self.id = id
        self.title = title
        self.detail = detail
        self.status = status
        self.kind = kind
        self.timestamp = timestamp
    }
}

public protocol TurnProcessServicing: Sendable {
    func fetchProcessNodes(sessionID: String, turnID: String) async throws -> [TurnProcessNode]
}
