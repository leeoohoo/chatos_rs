import ChatOSCore

struct AgentRuntimePreferencesTestProvider: AgentRuntimePreferencesProviding {
    var preferences = AgentRuntimePreferences()

    func load(ownerUserID: String) async throws -> AgentRuntimePreferences {
        preferences
    }
}
