import Testing
@testable import ChatOSCore

@Suite("ConversationTaskGraphAvailabilityState")
struct ConversationTaskGraphAvailabilityStateTests {
    @Test
    func newerRevisionRejectsOlderLookupForSameTurn() {
        var state = ConversationTaskGraphAvailabilityState()

        let old = state.begin(turnID: "turn-1", revision: 1)
        let current = state.begin(turnID: "turn-1", revision: 2)

        #expect(!state.accepts(old))
        #expect(state.accepts(current))
        #expect(current.generation == old.generation + 1)
    }

    @Test
    func differentTurnsRetainIndependentLookups() {
        var state = ConversationTaskGraphAvailabilityState()

        let first = state.begin(turnID: "turn-1", revision: 1)
        let second = state.begin(turnID: "turn-2", revision: 1)
        let firstFinished = state.finish(first)

        #expect(firstFinished)
        #expect(!state.accepts(first))
        #expect(state.accepts(second))
    }

    @Test
    func resetRejectsEveryLateLookup() {
        var state = ConversationTaskGraphAvailabilityState()
        let first = state.begin(turnID: "turn-1", revision: 1)
        let second = state.begin(turnID: "turn-2", revision: 1)

        state.reset()

        #expect(!state.accepts(first))
        #expect(!state.accepts(second))
        #expect(state.activeTokens.isEmpty)
    }
}
