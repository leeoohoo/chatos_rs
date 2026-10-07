import Foundation

public struct NativeInstalledAgentPlugin: Sendable, Equatable, Identifiable {
    public let id: String
    public let pluginKey: String
    public let displayName: String
    public let description: String
    public let componentCount: Int

    public init(
        id: String,
        pluginKey: String? = nil,
        displayName: String,
        description: String,
        componentCount: Int
    ) {
        self.id = id
        let normalizedPluginKey = pluginKey?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        self.pluginKey = (normalizedPluginKey?.isEmpty == false ? normalizedPluginKey : nil) ?? id
        self.displayName = displayName
        self.description = description
        self.componentCount = componentCount
    }
}
