import ChatOSCore
import Testing

struct LocalConnectorApprovalQueuePresentationStateTests {
    @Test
    func appearanceRecordsCurrentQueueAndExpandsOverlay() {
        var state = LocalConnectorApprovalQueuePresentationState()
        state.collapse()

        state.appear(approvalIDs: ["a", "b"])

        #expect(state.isExpanded)
        #expect(state.knownApprovalIDs == ["a", "b"])
        #expect(state.activeIndex == 0)
    }

    @Test
    func newApprovalReopensCollapsedOverlay() {
        var state = LocalConnectorApprovalQueuePresentationState()
        state.appear(approvalIDs: ["a"])
        state.collapse()

        state.update(approvalIDs: ["a", "b"])

        #expect(state.isExpanded)
        #expect(state.knownApprovalIDs == ["a", "b"])
    }

    @Test
    func removalsDoNotReopenOverlayAndClampActiveIndex() {
        var state = LocalConnectorApprovalQueuePresentationState()
        state.appear(approvalIDs: ["a", "b", "c"])
        state.moveToNext(approvalCount: 3)
        state.moveToNext(approvalCount: 3)
        state.collapse()

        state.update(approvalIDs: ["a"])

        #expect(!state.isExpanded)
        #expect(state.activeIndex == 0)
        #expect(state.knownApprovalIDs == ["a"])
    }

    @Test
    func navigationStaysInsideQueueBounds() {
        var state = LocalConnectorApprovalQueuePresentationState()

        state.moveToPrevious()
        #expect(state.activeIndex == 0)
        state.moveToNext(approvalCount: 2)
        state.moveToNext(approvalCount: 2)
        #expect(state.activeIndex == 1)
        state.moveToPrevious()
        #expect(state.activeIndex == 0)
    }

    @Test
    func activeApprovalFallsBackToFirstForOutOfRangeState() {
        var state = LocalConnectorApprovalQueuePresentationState()
        state.moveToNext(approvalCount: 3)
        state.moveToNext(approvalCount: 3)
        let approvals = [approval(id: "a")]

        #expect(state.activeApproval(in: approvals)?.id == "a")
        #expect(state.activeApproval(in: []) == nil)
    }

    private func approval(id: String) -> LocalConnectorPendingApproval {
        LocalConnectorPendingApproval(
            id: id,
            requestID: "request-\(id)",
            command: "git status",
            cwd: "/workspace",
            source: "task",
            risk: "low",
            reason: nil,
            createdAt: "2026-10-19T00:00:00Z",
            availableDecisions: ["decline", "accept"]
        )
    }
}
