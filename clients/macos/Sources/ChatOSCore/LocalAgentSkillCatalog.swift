import Foundation

public struct LocalAgentProfessionDefinition: Codable, Sendable, Equatable, Identifiable {
    public let key: String
    public let label: String
    public let labelEN: String
    public let description: String
    public let descriptionEN: String
    public let categoryKey: String
    public let categoryLabel: String
    public let categoryLabelEN: String
    public let skillName: String
    public let skillMarkdown: String
    public let skillMarkdownEN: String
    public let canCreateTasks: Bool

    public var id: String { key }

    enum CodingKeys: String, CodingKey {
        case key, label, description
        case labelEN = "label_en"
        case descriptionEN = "description_en"
        case categoryKey = "category_key"
        case categoryLabel = "category_label"
        case categoryLabelEN = "category_label_en"
        case skillName = "skill_name"
        case skillMarkdown = "skill_markdown"
        case skillMarkdownEN = "skill_markdown_en"
        case canCreateTasks = "can_create_tasks"
    }

    public var chatOSSkillName: String {
        "chatos-profession-" + key.replacingOccurrences(of: "_", with: "-")
    }
}

public struct LocalProjectTypeDefinition: Codable, Sendable, Equatable, Identifiable {
    public let key: String
    public let label: String
    public let labelEN: String
    public let description: String
    public let descriptionEN: String
    public let categoryKey: String
    public let categoryLabel: String
    public let categoryLabelEN: String
    public let ruleMarkdown: String
    public let ruleMarkdownEN: String

    public var id: String { key }

    enum CodingKeys: String, CodingKey {
        case key, label, description
        case labelEN = "label_en"
        case descriptionEN = "description_en"
        case categoryKey = "category_key"
        case categoryLabel = "category_label"
        case categoryLabelEN = "category_label_en"
        case ruleMarkdown = "rule_markdown"
        case ruleMarkdownEN = "rule_markdown_en"
    }

    public var skillName: String {
        "chatos-project-type-" + key.replacingOccurrences(of: "_", with: "-")
    }
}

/// Product-owned, immutable copy of Relay's complete profession and project-type catalogs.
/// Stable keys are persisted; models never provide or replace Skill markdown.
public enum LocalAgentSkillCatalog {
    public static let legacyProfessionKey = "general_member"
    public static let legacyProjectTypeKey = "software_development"

    private struct Payload: Codable {
        let professions: [LocalAgentProfessionDefinition]
        let projectTypes: [LocalProjectTypeDefinition]
    }

    private static let payload: Payload = {
        guard let url = Bundle.module.url(
            forResource: "RelaySkillCatalog",
            withExtension: "json"
        ), let data = try? Data(contentsOf: url),
           let value = try? JSONDecoder().decode(Payload.self, from: data),
           value.professions.count == 33,
           value.projectTypes.count == 27,
           Set(value.professions.map(\.key)).count == value.professions.count,
           Set(value.projectTypes.map(\.key)).count == value.projectTypes.count else {
            fatalError("Bundled Agent Skill catalog is missing or invalid")
        }
        return value
    }()

    public static var professions: [LocalAgentProfessionDefinition] { payload.professions }
    public static var projectTypes: [LocalProjectTypeDefinition] { payload.projectTypes }

    public static func profession(key: String) -> LocalAgentProfessionDefinition? {
        payload.professions.first { $0.key == key }
    }

    public static func projectType(key: String) -> LocalProjectTypeDefinition? {
        payload.projectTypes.first { $0.key == key }
    }

    public static func requireProfession(key: String) throws -> LocalAgentProfessionDefinition {
        guard let value = profession(key: key) else {
            throw AgentGroupChatError.invalidField("professionKey")
        }
        return value
    }

    public static func requireProjectType(key: String) throws -> LocalProjectTypeDefinition {
        guard let value = projectType(key: key) else {
            throw ProjectRegistryError.invalidField("projectTypeKey")
        }
        return value
    }
}
