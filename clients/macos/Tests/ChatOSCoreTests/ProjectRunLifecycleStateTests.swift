import Testing
@testable import ChatOSCore

@Suite("ProjectRunLifecycleState")
struct ProjectRunLifecycleStateTests {
    @Test
    func newerLoadRejectsOlderPresentationWrites() {
        var state = ProjectRunLifecycleState()
        let old = state.beginLoad(projectID: "project")!
        let current = state.beginLoad(projectID: "project")!

        #expect(!state.acceptsLoad(old))
        #expect(!state.acceptsState(old))
        #expect(state.acceptsLoad(current))
        #expect(state.acceptsState(current))

        state.finishLoad(old)
        #expect(state.isLoading)
        state.finishLoad(current)
        #expect(!state.isLoading)
    }

    @Test
    func realtimeStateRefreshSupersedesOnlySnapshotState() {
        var state = ProjectRunLifecycleState()
        let load = state.beginLoad(projectID: "project")!
        let realtime = state.beginStateRefresh(projectID: "project")!

        #expect(state.acceptsLoad(load))
        #expect(!state.acceptsState(load))
        #expect(state.acceptsState(realtime))
    }

    @Test
    func mutationIsExclusiveAndInvalidatesInflightReads() {
        var state = ProjectRunLifecycleState()
        let load = state.beginLoad(projectID: "project")!
        let mutation = state.beginMutation(projectID: "project")!

        #expect(!state.acceptsLoad(load))
        #expect(!state.acceptsState(load))
        #expect(state.acceptsMutation(mutation))
        #expect(state.beginLoad(projectID: "project") == nil)
        #expect(state.beginStateRefresh(projectID: "project") == nil)
        #expect(state.beginMutation(projectID: "project") == nil)

        state.finishMutation(mutation)
        #expect(!state.isMutating)
        #expect(state.beginLoad(projectID: "project") != nil)
    }

    @Test
    func resetRejectsAllLateResults() {
        var state = ProjectRunLifecycleState()
        let load = state.beginLoad(projectID: "project")!
        state.reset()

        #expect(!state.acceptsLoad(load))
        #expect(!state.acceptsState(load))
        #expect(!state.isLoading)
        #expect(!state.isMutating)
    }
}
