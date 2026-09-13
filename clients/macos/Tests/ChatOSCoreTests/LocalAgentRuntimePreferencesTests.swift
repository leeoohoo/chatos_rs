import ChatOSCore
import Testing

@Suite("Local Agent runtime preferences")
struct LocalAgentRuntimePreferencesTests {
    @Test("defaults match the shared runtime budgets")
    func defaults() throws {
        let preferences = AgentRuntimePreferences()

        #expect(preferences.effective(.story).maximumModelCalls == 600)
        #expect(preferences.effective(.approval).maximumModelCalls == 600)
        #expect(preferences.global.maximumRequestRetries == 5)

        let context = preferences.global.context ?? AgentContextPolicy()
        #expect(context.windowTokens == 250_000)
        #expect(context.outputReserveTokens == 30_000)
        #expect(context.compactionThresholdTokens == 220_000)
        #expect(context.maximumCompactionPasses == 8)
        #expect(context.summaryPollSeconds == 10)
        try preferences.validate()
    }

    @Test("profile overrides and invalid window budgets are validated")
    func validation() throws {
        var preferences = AgentRuntimePreferences()
        preferences.approvalMaximumCalls = 77
        preferences.storyMaximumCalls = 800
        preferences.global.context = .init()

        try preferences.validate()
        #expect(preferences.effective(.approval).maximumModelCalls == 77)
        #expect(preferences.effective(.story).maximumModelCalls == 800)

        preferences.global.context!.outputReserveTokens =
            preferences.global.context!.windowTokens
        #expect(throws: LocalAgentRuntimePreferencesError.self) {
            try preferences.validate()
        }
    }
}
