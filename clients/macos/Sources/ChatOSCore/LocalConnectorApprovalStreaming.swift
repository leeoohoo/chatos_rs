import Foundation

public enum LocalConnectorApprovalEventReviewer: String, Sendable, Equatable, Codable {
    case user
    case ai
    case policy
    case session
}

public struct LocalConnectorApprovalEvent: Identifiable, Sendable, Equatable, Codable {
    public var id: String
    public var requestID: String
    public var command: String
    public var cwd: String
    public var source: String
    public var risk: String
    public var decision: String
    public var reason: String?
    public var mode: LocalConnectorApprovalMode
    public var reviewer: LocalConnectorApprovalEventReviewer
    public var createdAt: Date

    public init(
        id: String = UUID().uuidString,
        requestID: String,
        command: String,
        cwd: String,
        source: String,
        risk: String,
        decision: String,
        reason: String?,
        mode: LocalConnectorApprovalMode,
        reviewer: LocalConnectorApprovalEventReviewer,
        createdAt: Date = Date()
    ) {
        self.id = id
        self.requestID = requestID
        self.command = command
        self.cwd = cwd
        self.source = source
        self.risk = risk
        self.decision = decision
        self.reason = reason
        self.mode = mode
        self.reviewer = reviewer
        self.createdAt = createdAt
    }
}

public protocol LocalConnectorApprovalStreaming: Sendable {
    func approvalSnapshots() async -> AsyncStream<[LocalConnectorPendingApproval]>
    func approvalEvents() async -> AsyncStream<LocalConnectorApprovalEvent>
}
