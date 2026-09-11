import ChatOSAgentRuntime
import Foundation

/// Global inventory of registered prompt templates and current Agent tool contracts.
/// Runtime story data is deliberately excluded from this configuration surface.
enum StoryPromptCatalog {
    typealias Category = StoryPromptRegistry.Category

    struct Item: Identifiable {
        let id: String
        let registryKey: String
        let category: Category
        let title: String
        let usedWhen: String
        let trigger: String
        let implementation: String
        let content: String?
        let unavailableReason: String?
    }

    static func items() -> [Item] {
        var items = StoryPromptRegistry.definitions.map { definition in
            Item(
                id: definition.key.rawValue, registryKey: definition.key.rawValue,
                category: definition.category, title: definition.title,
                usedWhen: definition.usedWhen, trigger: definition.trigger,
                implementation: definition.implementation,
                content: StoryPromptRegistry.template(definition.key), unavailableReason: nil
            )
        }
        if let definitions = try? StoryAgentTools.definitions(stage: .outline) {
            items += definitions.map(toolItem)
        }
        if let definitions = try? StoryAgentTools.definitions(stage: .refine) {
            let existing = Set(items.map(\.id))
            items += definitions.map(toolItem).filter { !existing.contains($0.id) }
        }
        return items
    }

    private static func toolItem(_ definition: AgentToolDefinition) -> Item {
        let schema: String
        if let object = try? JSONSerialization.jsonObject(with: definition.schema),
           let data = try? JSONSerialization.data(withJSONObject: object, options: [.prettyPrinted, .sortedKeys]) {
            schema = String(decoding: data, as: UTF8.self)
        } else {
            schema = String(decoding: definition.schema, as: UTF8.self)
        }
        return .init(
            id: "story.tool.\(definition.name)", registryKey: "story.tool.\(definition.name)",
            category: .tools, title: "工具 · \(definition.name)", usedWhen: definition.description,
            trigger: "剧情规划 Agent 根据当前阶段调用",
            implementation: "StoryAgentTools.swift · definitions / execute",
            content: "【Description】\n\(definition.description)\n\n【JSON Schema】\n\(schema)",
            unavailableReason: nil
        )
    }
}
