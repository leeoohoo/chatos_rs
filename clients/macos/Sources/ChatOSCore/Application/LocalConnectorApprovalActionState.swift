import Foundation

public struct LocalConnectorApprovalActionToken: Sendable, Equatable {
    public let approvalID: String
    public let requestID: UUID

    public init(approvalID: String, requestID: UUID) {
        self.approvalID = approvalID
        self.requestID = requestID
    }
}

/// Pure identity state for approval mutations. A decision is exclusive per approval,
/// while unrelated approvals and other control-center actions remain independent.
public struct LocalConnectorApprovalActionState: Sendable, Equatable {
    private var requestIDsByApprovalID: [String: UUID] = [:]

    public init() {}

    public var activeApprovalIDs: Set<String> {
        Set(requestIDsByApprovalID.keys)
    }

    public mutating func begin(approvalID: String) -> LocalConnectorApprovalActionToken? {
        guard requestIDsByApprovalID[approvalID] == nil else { return nil }
        let requestID = UUID()
        requestIDsByApprovalID[approvalID] = requestID
        return .init(approvalID: approvalID, requestID: requestID)
    }

    public func accepts(_ token: LocalConnectorApprovalActionToken) -> Bool {
        requestIDsByApprovalID[token.approvalID] == token.requestID
    }

    @discardableResult
    public mutating func finish(_ token: LocalConnectorApprovalActionToken) -> Bool {
        guard accepts(token) else { return false }
        requestIDsByApprovalID.removeValue(forKey: token.approvalID)
        return true
    }

    public mutating func reset() {
        requestIDsByApprovalID.removeAll()
    }
}
