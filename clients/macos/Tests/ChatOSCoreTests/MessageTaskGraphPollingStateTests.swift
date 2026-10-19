import ChatOSCore
import Foundation
import Testing

struct MessageTaskGraphPollingStateTests {
    @Test
    func emptyGraphRecoveryStopsAfterThreeAttempts() {
        var state = MessageTaskGraphPollingState()
        let graph = emptyGraph

        let isAwaiting = state.observe(graph: graph, expectsTaskGraph: true)
        let initialRemaining = state.emptyGraphRetryAttemptsRemaining
        let firstRetry = state.recordEmptyGraphRetry(expectsTaskGraph: true, graph: graph)
        let secondRetry = state.recordEmptyGraphRetry(expectsTaskGraph: true, graph: graph)
        let thirdRetry = state.recordEmptyGraphRetry(expectsTaskGraph: true, graph: graph)

        #expect(isAwaiting)
        #expect(initialRemaining == 3)
        #expect(firstRetry == true)
        #expect(secondRetry == true)
        #expect(thirdRetry == false)
        #expect(state.emptyGraphRetryAttemptsRemaining == 0)
        #expect(!state.shouldPoll(graph: graph, expectsTaskGraph: true))
    }

    @Test
    func explicitRefreshRestoresEmptyGraphBudgetOnlyWhenExpected() {
        var state = MessageTaskGraphPollingState(emptyGraphRetryAttemptsRemaining: 0)

        let ignoredReset = state.resetRetryBudgetIfNeeded(
            expectsTaskGraph: false,
            graph: emptyGraph
        )
        #expect(ignoredReset == nil)
        #expect(state.emptyGraphRetryAttemptsRemaining == 0)
        let appliedReset = state.resetRetryBudgetIfNeeded(
            expectsTaskGraph: true,
            graph: emptyGraph
        )
        #expect(appliedReset == true)
        #expect(state.emptyGraphRetryAttemptsRemaining == 3)
    }

    @Test
    func populatedGraphStopsRecoveryButPollsWhileTaskIsActive() {
        var state = MessageTaskGraphPollingState()
        let running = graph(status: "running")

        let isAwaiting = state.observe(graph: running, expectsTaskGraph: true)
        #expect(!isAwaiting)
        #expect(state.emptyGraphRetryAttemptsRemaining == 0)
        #expect(state.shouldPoll(graph: running, expectsTaskGraph: true))
        #expect(!state.shouldPoll(
            graph: graph(status: "completed"),
            expectsTaskGraph: true
        ))
    }

    @Test
    func pollingIntervalsPreserveMacPerformancePolicy() {
        #expect(MessageTaskGraphPollingState.interval(
            isEmptyGraphRetry: true,
            hasActiveRealtimeStream: true
        ) == .milliseconds(600))
        #expect(MessageTaskGraphPollingState.interval(
            isEmptyGraphRetry: false,
            hasActiveRealtimeStream: false
        ) == .seconds(2))
        #expect(MessageTaskGraphPollingState.interval(
            isEmptyGraphRetry: false,
            hasActiveRealtimeStream: true
        ) == .seconds(60))
    }

    private var emptyGraph: MessageTaskGraphSnapshot {
        MessageTaskGraphSnapshot(rootTaskIDs: [], nodes: [], edges: [])
    }

    private func graph(status: String) -> MessageTaskGraphSnapshot {
        MessageTaskGraphSnapshot(
            rootTaskIDs: ["task-1"],
            nodes: [
                MessageTaskGraphNode(
                    task: MessageTask(id: "task-1", title: "Task", status: status),
                    depth: 0,
                    isRoot: true,
                    isCurrentMessage: true
                ),
            ],
            edges: []
        )
    }
}
