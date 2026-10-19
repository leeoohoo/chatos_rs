import ChatOSCore
import Testing

struct ConversationLatestRefreshStateTests {
    @Test
    func debounceKeepsVisiblePresentationWhenSignalsCoalesce() {
        var state = ConversationLatestRefreshState()

        let first = state.scheduleDebounced(.silent)
        let second = state.scheduleDebounced(.visible)
        let third = state.scheduleDebounced(.silent)
        #expect(first == .silent)
        #expect(second == .visible)
        #expect(third == .visible)
        #expect(state.scheduledPresentation == .visible)

        state.consumeDebounced()
        #expect(state.scheduledPresentation == nil)
    }

    @Test
    func inFlightRefreshQueuesOneMergedFollowUp() {
        var state = ConversationLatestRefreshState()

        let began = state.beginOrQueue(.silent)
        let queuedSilent = state.beginOrQueue(.silent)
        let queuedVisible = state.beginOrQueue(.visible)
        #expect(began)
        #expect(!queuedSilent)
        #expect(!queuedVisible)
        #expect(state.isInFlight)
        #expect(state.pendingPresentation == .visible)

        let pending = state.complete()
        #expect(pending == .visible)
        #expect(!state.isInFlight)
        #expect(state.pendingPresentation == nil)
    }

    @Test
    func completingWithoutPendingRefreshReturnsNothing() {
        var state = ConversationLatestRefreshState()

        let began = state.beginOrQueue(.visible)
        let pending = state.complete()
        #expect(began)
        #expect(pending == nil)
        #expect(!state.isInFlight)
    }

    @Test
    func lifecycleCancellationDropsScheduledAndPendingButPreservesInFlight() {
        var state = ConversationLatestRefreshState()
        _ = state.scheduleDebounced(.silent)
        let began = state.beginOrQueue(.silent)
        let queued = state.beginOrQueue(.visible)
        #expect(began)
        #expect(!queued)

        state.cancelScheduledAndPending()

        #expect(state.scheduledPresentation == nil)
        #expect(state.pendingPresentation == nil)
        #expect(state.isInFlight)
    }

    @Test
    func debounceDelayMatchesMacOSBehavior() {
        #expect(ConversationLatestRefreshState.debounceDelay == .milliseconds(250))
    }
}
