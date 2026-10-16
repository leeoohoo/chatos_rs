import Foundation

public struct ConversationExplicitCapabilitySelection: Sendable, Equatable, Codable {
    public enum Kind: String, Sendable, Codable {
        case plugin
        case requirementSurvey = "requirement_survey"
    }

    public var kind: Kind
    public var pluginKey: String?
    public var componentKey: String?
    public var displayName: String

    public init(kind: Kind, pluginKey: String? = nil, componentKey: String? = nil, displayName: String) {
        self.kind = kind
        self.pluginKey = pluginKey
        self.componentKey = componentKey
        self.displayName = displayName
    }

    public static func plugin(key: String, componentKey: String, displayName: String) -> Self {
        .init(kind: .plugin, pluginKey: key, componentKey: componentKey, displayName: displayName)
    }

    public static func requirementSurvey(displayName: String = "需求调研") -> Self {
        .init(kind: .requirementSurvey, displayName: displayName)
    }
}
