import Foundation
import Testing
@testable import ChatOSCore

@Suite("Plugin application launch")
struct PluginApplicationLaunchUseCaseTests {
    @Test
    func projectSelectionIsResolvedBeforeLaunchAndPathStaysOutOfPresentation() async throws {
        let context = PluginApplicationContextStub(context: .init(
            projectID: "project-1",
            projectName: "Project",
            projectRoot: "chatos-workspace://device/workspace/root"
        ))
        let launcher = PluginApplicationLauncherStub()
        let useCase = PluginApplicationLaunchUseCase(
            contextProvider: context,
            launcher: launcher
        )

        let result = try await useCase.launch(
            application: application(contextScope: "project"),
            ownerUserID: " owner-1 ",
            context: .project(id: " project-1 ")
        )

        #expect(result.bridgeContext == .init(
            projectID: "project-1",
            projectName: "Project"
        ))
        #expect(result.launch.application.id == "plugin:main")
        #expect(await context.requests == [.init(
            ownerUserID: "owner-1",
            projectID: "project-1"
        )])
        #expect(await launcher.requests == [.init(
            pluginID: "plugin",
            componentKey: "main",
            context: .init(
                projectID: "project-1",
                projectName: "Project",
                projectRoot: "chatos-workspace://device/workspace/root"
            ),
            ownerUserID: "owner-1"
        )])
    }

    @Test
    func contextPolicyRejectsMissingOrInventedSelections() async {
        let useCase = PluginApplicationLaunchUseCase(
            contextProvider: PluginApplicationContextStub(context: .device),
            launcher: PluginApplicationLauncherStub()
        )

        await #expect(throws: PluginApplicationLaunchError.contextRequired) {
            try await useCase.launch(
                application: application(contextScope: "workspace"),
                ownerUserID: "owner",
                context: .none
            )
        }
        await #expect(throws: PluginApplicationLaunchError.invalidContextSelection) {
            try await useCase.launch(
                application: application(),
                ownerUserID: "owner",
                context: .project(id: "project")
            )
        }
        await #expect(throws: PluginApplicationLaunchError.invalidContextSelection) {
            try await useCase.launch(
                application: application(contextScope: "project", missingContext: nil),
                ownerUserID: "owner",
                context: .device
            )
        }
    }

    @Test
    func newerRequestAndResetRejectOlderLaunchTokens() {
        var state = PluginApplicationLaunchState()
        let old = state.begin(
            ownerUserID: "owner",
            accountGeneration: 1,
            applicationID: "plugin:main",
            selection: .project(id: "old")
        )
        let current = state.begin(
            ownerUserID: "owner",
            accountGeneration: 1,
            applicationID: "plugin:main",
            selection: .project(id: "current")
        )

        #expect(!state.accepts(
            old,
            ownerUserID: "owner",
            accountGeneration: 1,
            applicationID: "plugin:main"
        ))
        #expect(state.accepts(
            current,
            ownerUserID: "owner",
            accountGeneration: 1,
            applicationID: "plugin:main"
        ))
        #expect(!state.accepts(
            current,
            ownerUserID: "other",
            accountGeneration: 2,
            applicationID: "plugin:main"
        ))
        state.reset()
        #expect(!state.accepts(
            current,
            ownerUserID: "owner",
            accountGeneration: 1,
            applicationID: "plugin:main"
        ))
    }
}

private struct PluginApplicationContextRequest: Sendable, Equatable {
    let ownerUserID: String
    let projectID: String
}

private actor PluginApplicationContextStub: PluginApplicationContextProviding {
    let context: LocalConnectorPluginApplicationContext
    private(set) var requests: [PluginApplicationContextRequest] = []

    init(context: LocalConnectorPluginApplicationContext) {
        self.context = context
    }

    func pluginContext(
        ownerUserID: String,
        projectID: String
    ) async throws -> LocalConnectorPluginApplicationContext {
        requests.append(.init(ownerUserID: ownerUserID, projectID: projectID))
        return context
    }
}

private struct PluginApplicationLaunchRequest: Sendable, Equatable {
    let pluginID: String
    let componentKey: String
    let context: LocalConnectorPluginApplicationContext?
    let ownerUserID: String
}

private actor PluginApplicationLauncherStub: LocalConnectorPluginApplicationLaunching {
    private(set) var requests: [PluginApplicationLaunchRequest] = []

    func launchPluginApplication(
        pluginID: String,
        componentKey: String,
        context: LocalConnectorPluginApplicationContext?,
        expectedOwnerUserID: String
    ) async throws -> LocalConnectorPluginApplicationLaunch {
        requests.append(.init(
            pluginID: pluginID,
            componentKey: componentKey,
            context: context,
            ownerUserID: expectedOwnerUserID
        ))
        return .init(
            application: application(),
            url: URL(string: "https://plugin.invalid")!
        )
    }
}

private func application(
    contextScope: String? = nil,
    missingContext: String? = "device"
) -> LocalConnectorPluginApplication {
    .init(
        pluginID: "plugin",
        componentKey: "main",
        displayName: "Plugin",
        description: "",
        requiresLocalRuntime: true,
        contextScope: contextScope,
        missingContext: missingContext,
        bridgeCapabilities: ["host.context.read"]
    )
}
