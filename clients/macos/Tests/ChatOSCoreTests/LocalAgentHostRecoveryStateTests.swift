import Testing
@testable import ChatOSCore

@Suite("Local Agent Host recovery lifecycle")
struct LocalAgentHostRecoveryStateTests {
    @Test
    func healthCheckRequiresMatchingRequestAccountAndLifecycle() {
        var state = LocalAgentHostRecoveryState()
        let old = state.beginHealthCheck(
            ownerUserID: "owner-1",
            accountGeneration: 1,
            lifecycleGeneration: 10
        )
        let current = state.beginHealthCheck(
            ownerUserID: "owner-1",
            accountGeneration: 1,
            lifecycleGeneration: 10
        )

        #expect(!state.acceptsHealthCheck(
            old,
            ownerUserID: "owner-1",
            accountGeneration: 1,
            lifecycleGeneration: 10
        ))
        #expect(state.acceptsHealthCheck(
            current,
            ownerUserID: "owner-1",
            accountGeneration: 1,
            lifecycleGeneration: 10
        ))
        #expect(!state.acceptsHealthCheck(
            current,
            ownerUserID: "owner-2",
            accountGeneration: 2,
            lifecycleGeneration: 11
        ))
    }

    @Test
    func delayedRestartCannotCrossAccountOrHostLifecycle() throws {
        var state = LocalAgentHostRecoveryState()
        let scheduledPlan = state.scheduleRestart(
            ownerUserID: "owner-1",
            accountGeneration: 4,
            lifecycleGeneration: 8
        )
        let plan = try #require(scheduledPlan)

        #expect(plan.token.attempt == 1)
        #expect(plan.delay == .milliseconds(250))
        #expect(state.acceptsRestart(
            plan.token,
            ownerUserID: "owner-1",
            accountGeneration: 4,
            lifecycleGeneration: 8
        ))
        #expect(!state.acceptsRestart(
            plan.token,
            ownerUserID: "owner-1",
            accountGeneration: 4,
            lifecycleGeneration: 9
        ))
        #expect(!state.acceptsRestart(
            plan.token,
            ownerUserID: "owner-2",
            accountGeneration: 5,
            lifecycleGeneration: 8
        ))
    }

    @Test
    func restartBackoffIsBoundedAndResettable() throws {
        var state = LocalAgentHostRecoveryState()
        let expectedDelays: [Duration] = [
            .milliseconds(250),
            .seconds(1),
            .seconds(2),
            .seconds(5),
            .seconds(10),
        ]

        for (index, expectedDelay) in expectedDelays.enumerated() {
            let scheduledPlan = state.scheduleRestart(
                ownerUserID: "owner-1",
                accountGeneration: 1,
                lifecycleGeneration: 3
            )
            let plan = try #require(scheduledPlan)
            #expect(plan.token.attempt == index + 1)
            #expect(plan.delay == expectedDelay)
            let didFinish = state.finishRestart(plan.token)
            #expect(didFinish)
        }

        #expect(state.hasExhaustedRestarts)
        #expect(state.scheduleRestart(
            ownerUserID: "owner-1",
            accountGeneration: 1,
            lifecycleGeneration: 3
        ) == nil)

        state.resetRestarts()
        let scheduledResetPlan = state.scheduleRestart(
            ownerUserID: "owner-1",
            accountGeneration: 1,
            lifecycleGeneration: 4
        )
        let resetPlan = try #require(scheduledResetPlan)
        #expect(resetPlan.token.attempt == 1)
        #expect(resetPlan.delay == .milliseconds(250))
    }

    @Test
    func cancelledTokensCannotFinishNewOperations() {
        var state = LocalAgentHostRecoveryState()
        let health = state.beginHealthCheck(
            ownerUserID: "owner-1",
            accountGeneration: 1,
            lifecycleGeneration: 1
        )
        state.cancelHealthCheck()
        let didFinishHealthCheck = state.finishHealthCheck(health)
        #expect(!didFinishHealthCheck)

        let restart = state.scheduleRestart(
            ownerUserID: "owner-1",
            accountGeneration: 1,
            lifecycleGeneration: 1
        )
        state.reset()
        let didFinishRestart = state.finishRestart(restart!.token)
        #expect(!didFinishRestart)
        #expect(state.recoveryAttempts == 0)
    }
}
