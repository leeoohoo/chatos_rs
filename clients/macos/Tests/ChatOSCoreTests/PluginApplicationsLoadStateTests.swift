import Testing
@testable import ChatOSCore

@Suite("Plugin applications load")
struct PluginApplicationsLoadStateTests {
    @Test
    func newerRequestAndAccountChangeRejectOlderToken() {
        var state = PluginApplicationsLoadState()
        let old = state.begin(ownerUserID: "owner-1", accountGeneration: 1)
        let current = state.begin(ownerUserID: "owner-1", accountGeneration: 1)

        #expect(!state.accepts(
            old,
            ownerUserID: "owner-1",
            accountGeneration: 1
        ))
        #expect(state.accepts(
            current,
            ownerUserID: "owner-1",
            accountGeneration: 1
        ))
        #expect(!state.accepts(
            current,
            ownerUserID: "owner-2",
            accountGeneration: 2
        ))
    }

    @Test
    func resetRejectsActiveRequest() {
        var state = PluginApplicationsLoadState()
        let token = state.begin(ownerUserID: "owner-1", accountGeneration: 1)

        state.reset()

        #expect(!state.accepts(
            token,
            ownerUserID: "owner-1",
            accountGeneration: 1
        ))
    }

    @Test
    func useCasePreservesConnectorApplicationOrder() async throws {
        let applications = [
            LocalConnectorPluginApplication(
                pluginID: "plugin-b",
                componentKey: "workbench",
                displayName: "B",
                description: "B application",
                requiresLocalRuntime: true
            ),
            LocalConnectorPluginApplication(
                pluginID: "plugin-a",
                componentKey: "workbench",
                displayName: "A",
                description: "A application",
                requiresLocalRuntime: false
            ),
        ]
        let service = PluginApplicationListStub(applications: applications)
        let useCase = PluginApplicationListUseCase(service: service)

        let loaded = try await useCase.load()

        #expect(loaded == applications)
    }
}

private actor PluginApplicationListStub: LocalConnectorPluginApplicationListing {
    let applications: [LocalConnectorPluginApplication]

    init(applications: [LocalConnectorPluginApplication]) {
        self.applications = applications
    }

    func fetchPluginApplications() async throws -> [LocalConnectorPluginApplication] {
        applications
    }
}
