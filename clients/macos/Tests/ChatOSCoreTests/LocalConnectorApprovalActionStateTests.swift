import Testing
@testable import ChatOSCore

@Suite("LocalConnectorApprovalActionState")
struct LocalConnectorApprovalActionStateTests {
    @Test
    func oneApprovalCannotStartTwoConcurrentDecisions() throws {
        var state = LocalConnectorApprovalActionState()

        let started = state.begin(approvalID: "approval-1")
        let token = try #require(started)

        #expect(state.begin(approvalID: "approval-1") == nil)
        #expect(state.accepts(token))
        #expect(state.activeApprovalIDs == ["approval-1"])
    }

    @Test
    func differentApprovalsRemainIndependent() throws {
        var state = LocalConnectorApprovalActionState()
        let firstStarted = state.begin(approvalID: "approval-1")
        let secondStarted = state.begin(approvalID: "approval-2")
        let first = try #require(firstStarted)
        let second = try #require(secondStarted)

        let didFinish = state.finish(first)
        #expect(didFinish)
        #expect(!state.accepts(first))
        #expect(state.accepts(second))
        #expect(state.activeApprovalIDs == ["approval-2"])
    }

    @Test
    func resetRejectsLateCompletion() throws {
        var state = LocalConnectorApprovalActionState()
        let started = state.begin(approvalID: "approval-1")
        let token = try #require(started)

        state.reset()

        #expect(!state.accepts(token))
        let didFinish = state.finish(token)
        #expect(!didFinish)
        #expect(state.activeApprovalIDs.isEmpty)
    }
}
