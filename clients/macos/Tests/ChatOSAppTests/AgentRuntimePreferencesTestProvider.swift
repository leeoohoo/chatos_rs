import ChatOSCore

actor AppAgentRuntimePreferencesTestProvider: AgentRuntimePreferencesProviding {
    private var preferences: AgentRuntimePreferences

    init(_ preferences: AgentRuntimePreferences = .init()) {
        self.preferences = preferences
    }

    func load(ownerUserID: String) async throws -> AgentRuntimePreferences {
        preferences
    }

    func replace(_ preferences: AgentRuntimePreferences) {
        self.preferences = preferences
    }
}
