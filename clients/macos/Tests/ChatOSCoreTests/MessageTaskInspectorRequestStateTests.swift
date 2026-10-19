import Testing
@testable import ChatOSCore

@Suite("MessageTaskInspectorRequestState")
struct MessageTaskInspectorRequestStateTests {
    @Test
    func newerSnapshotRejectsOlderRequestForSameTask() {
        var state = MessageTaskInspectorRequestState()

        let old = state.beginSnapshot(contextID: "task-1:detail")
        let current = state.beginSnapshot(contextID: "task-1:detail")

        #expect(current.generation == old.generation + 1)
        #expect(!state.accepts(old))
        #expect(state.accepts(current))
    }

    @Test
    func snapshotInvalidatesPaginationFromPreviousRunState() {
        var state = MessageTaskInspectorRequestState()
        let page = state.beginOlderEvents(contextID: "run-1:20")

        let snapshot = state.beginSnapshot(contextID: "task-1:run")

        #expect(!state.accepts(page))
        #expect(state.accepts(snapshot))
    }

    @Test
    func paginationDoesNotInvalidateCurrentSnapshot() {
        var state = MessageTaskInspectorRequestState()
        let snapshot = state.beginSnapshot(contextID: "task-1:run")
        let page = state.beginOlderEvents(contextID: "run-1:20")

        #expect(state.accepts(snapshot))
        #expect(state.accepts(page))
    }

    @Test
    func staleRequestCannotFinishCurrentLoadingOwner() {
        var state = MessageTaskInspectorRequestState()
        let old = state.beginSnapshot(contextID: "task-1:detail")
        let current = state.beginSnapshot(contextID: "task-1:run")

        let didFinishOld = state.finish(old)
        let didFinishCurrent = state.finish(current)

        #expect(!didFinishOld)
        #expect(didFinishCurrent)
        #expect(!state.accepts(current))
    }

    @Test
    func resetRejectsAllLateResults() {
        var state = MessageTaskInspectorRequestState()
        let snapshot = state.beginSnapshot(contextID: "task-1:run")
        let page = state.beginOlderEvents(contextID: "run-1:20")

        state.reset()

        #expect(!state.accepts(snapshot))
        #expect(!state.accepts(page))
    }
}
