import Foundation

public struct NativeInstalledAgentPlugin: Sendable, Equatable, Identifiable {
    public static let skillsOnlyComponentKey = "plugin-skills"

    public let pluginID: String
    public let pluginKey: String
    public let componentKey: String
    public let displayName: String
    public let description: String

    public var id: String { "\(pluginID):\(componentKey)" }

    public init(
        pluginID: String,
        pluginKey: String? = nil,
        componentKey: String,
        displayName: String,
        description: String
    ) {
        self.pluginID = pluginID
        let normalizedPluginKey = pluginKey?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        self.pluginKey = (normalizedPluginKey?.isEmpty == false ? normalizedPluginKey : nil) ?? pluginID
        self.componentKey = componentKey
        self.displayName = displayName
        self.description = description
    }
}
