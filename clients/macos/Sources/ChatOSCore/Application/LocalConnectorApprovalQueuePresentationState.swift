public struct LocalConnectorApprovalQueuePresentationState: Sendable, Equatable {
    public private(set) var isExpanded = true
    public private(set) var activeIndex = 0
    public private(set) var knownApprovalIDs: Set<String> = []

    public init() {}

    public mutating func appear(approvalIDs: [String]) {
        knownApprovalIDs = Set(approvalIDs)
        isExpanded = true
        clampActiveIndex(approvalCount: approvalIDs.count)
    }

    public mutating func update(approvalIDs: [String]) {
        let nextIDs = Set(approvalIDs)
        if !nextIDs.subtracting(knownApprovalIDs).isEmpty {
            isExpanded = true
        }
        knownApprovalIDs = nextIDs
        clampActiveIndex(approvalCount: approvalIDs.count)
    }

    public mutating func expand() {
        isExpanded = true
    }

    public mutating func collapse() {
        isExpanded = false
    }

    public mutating func moveToPrevious() {
        activeIndex = max(0, activeIndex - 1)
    }

    public mutating func moveToNext(approvalCount: Int) {
        activeIndex = min(max(0, approvalCount - 1), activeIndex + 1)
    }

    public func activeApproval(
        in approvals: [LocalConnectorPendingApproval]
    ) -> LocalConnectorPendingApproval? {
        guard approvals.indices.contains(activeIndex) else {
            return approvals.first
        }
        return approvals[activeIndex]
    }

    private mutating func clampActiveIndex(approvalCount: Int) {
        activeIndex = min(activeIndex, max(0, approvalCount - 1))
    }
}
