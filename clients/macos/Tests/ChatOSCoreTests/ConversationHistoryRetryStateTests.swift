import Foundation
import Testing
@testable import ChatOSCore

@Suite("Conversation history retry state")
struct ConversationHistoryRetryStateTests {
    @Test
    func schedulesBoundedBackoffForOneActivation() throws {
        var state = ConversationHistoryRetryState()
        let activation = ConversationSessionActivation(
            generation: 1,
            requestID: UUID()
        )
        var delays: [Duration] = []

        for _ in 0..<4 {
            let pendingSchedule = state.schedule(activation: activation)
            let schedule = try #require(pendingSchedule)
            delays.append(schedule.delay)
            #expect(state.accepts(schedule.token, activation: activation))
            let didFinish = state.finish(schedule.token)
            #expect(didFinish)
        }

        #expect(delays == [.seconds(2), .seconds(5), .seconds(10), .seconds(20)])
        #expect(state.schedule(activation: activation) == nil)
    }

    @Test
    func activationChangeAndCancellationRejectScheduledRetry() throws {
        var state = ConversationHistoryRetryState()
        let oldActivation = ConversationSessionActivation(
            generation: 1,
            requestID: UUID()
        )
        let newActivation = ConversationSessionActivation(
            generation: 2,
            requestID: UUID()
        )
        let pendingSchedule = state.schedule(activation: oldActivation)
        let schedule = try #require(pendingSchedule)

        #expect(!state.accepts(schedule.token, activation: newActivation))

        state.cancelScheduled()

        #expect(!state.accepts(schedule.token, activation: oldActivation))
        #expect(state.nextAttempt == 1)
    }

    @Test
    func resetRestoresInitialRetryBudget() throws {
        var state = ConversationHistoryRetryState()
        let activation = ConversationSessionActivation(
            generation: 1,
            requestID: UUID()
        )
        let pendingFirst = state.schedule(activation: activation)
        let first = try #require(pendingFirst)
        let didFinishFirst = state.finish(first.token)
        #expect(didFinishFirst)
        let pendingSecond = state.schedule(activation: activation)
        let second = try #require(pendingSecond)
        #expect(second.delay == .seconds(5))

        state.reset()
        let pendingRestarted = state.schedule(activation: activation)
        let restarted = try #require(pendingRestarted)

        #expect(restarted.delay == .seconds(2))
    }
}
