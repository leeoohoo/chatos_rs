import ChatOSCore
import Foundation

enum PluginApplicationLaunchRecovery {
    static func refreshedContext(
        _ context: LocalConnectorPluginApplicationContext?,
        projects: [WorkspaceProject]
    ) -> LocalConnectorPluginApplicationContext? {
        guard let context,
              let projectID = context.projectID?.pluginLaunchValue,
              let project = projects.first(where: { $0.id == projectID }) else {
            return context
        }
        return LocalConnectorPluginApplicationContext(
            projectID: project.id,
            projectName: project.name,
            projectRoot: project.rootPath
        )
    }

    static func allowsProjectOnlyFallback(_ application: LocalConnectorPluginApplication) -> Bool {
        application.missingContext == "device"
    }

    static func projectOnlyContext(
        _ context: LocalConnectorPluginApplicationContext?
    ) -> LocalConnectorPluginApplicationContext? {
        guard let context,
              context.projectID?.pluginLaunchValue != nil else { return nil }
        return LocalConnectorPluginApplicationContext(
            projectID: context.projectID,
            projectName: context.projectName,
            projectRoot: nil
        )
    }
}

extension String {
    var pluginLaunchValue: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
