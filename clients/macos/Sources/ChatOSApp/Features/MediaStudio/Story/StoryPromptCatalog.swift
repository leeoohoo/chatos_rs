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
        StoryPromptRegistry.definitions.map { definition in
            Item(
                id: definition.key.rawValue, registryKey: definition.key.rawValue,
                category: definition.category, title: definition.title,
                usedWhen: definition.usedWhen, trigger: definition.trigger,
                implementation: definition.implementation,
                content: StoryPromptRegistry.template(definition.key), unavailableReason: nil
            )
        }
    }
}
