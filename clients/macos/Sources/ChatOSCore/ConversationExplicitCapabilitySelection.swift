import Foundation

public struct ConversationExplicitCapabilitySelection: Sendable, Equatable, Codable {
    public enum Kind: String, Sendable, Codable {
        case plugin
        case requirementSurvey = "requirement_survey"
    }

    public var kind: Kind
    public var pluginKey: String?
    public var displayName: String

    public init(kind: Kind, pluginKey: String? = nil, displayName: String) {
        self.kind = kind
        self.pluginKey = pluginKey
        self.displayName = displayName
    }

    public static func plugin(key: String, displayName: String) -> Self {
        .init(kind: .plugin, pluginKey: key, displayName: displayName)
    }

    public static func requirementSurvey(displayName: String = "需求调研") -> Self {
        .init(kind: .requirementSurvey, displayName: displayName)
    }
}
