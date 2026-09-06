import ChatOSCore
import Testing
@testable import ChatOSApp

@Suite("Plugin application launch recovery")
struct PluginApplicationLaunchRecoveryTests {
    @Test("uses the latest project path after refreshing the workspace snapshot")
    func refreshesProjectContext() {
        let stale = LocalConnectorPluginApplicationContext(
            projectID: "project-1",
            projectName: "Old name",
            projectRoot: "local://connector/old-device/old-workspace/project"
        )
        let refreshed = PluginApplicationLaunchRecovery.refreshedContext(
            stale,
            projects: [
                WorkspaceProject(
                    id: "project-1",
                    name: "Current name",
                    rootPath: "local://connector/device-1/workspace-1/project",
                    latestConversationID: nil
                ),
            ]
        )

        #expect(refreshed?.projectName == "Current name")
        #expect(refreshed?.projectRoot == "local://connector/device-1/workspace-1/project")
    }

    @Test("keeps project isolation when an optional workspace is unavailable")
    func buildsProjectOnlyFallback() {
        let context = LocalConnectorPluginApplicationContext(
            projectID: "project-1",
            projectName: "Project One",
            projectRoot: "local://connector/stale/stale/project"
        )

        let fallback = PluginApplicationLaunchRecovery.projectOnlyContext(context)

        #expect(fallback?.projectID == "project-1")
        #expect(fallback?.projectName == "Project One")
        #expect(fallback?.projectRoot == nil)
    }

    @Test("only plugins declaring device fallback may omit an unavailable workspace")
    func respectsMissingContextPolicy() {
        let optionalWorkspace = application(missingContext: "device")
        let requiredWorkspace = application(missingContext: "reject")

        #expect(PluginApplicationLaunchRecovery.allowsProjectOnlyFallback(optionalWorkspace))
        #expect(!PluginApplicationLaunchRecovery.allowsProjectOnlyFallback(requiredWorkspace))
    }

    private func application(missingContext: String) -> LocalConnectorPluginApplication {
        LocalConnectorPluginApplication(
            pluginID: "plugin-1",
            componentKey: "workbench",
            displayName: "Fixture",
            description: "Fixture",
            requiresLocalRuntime: true,
            contextScope: "project",
            missingContext: missingContext
        )
    }
}
