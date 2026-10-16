import Foundation

/// Shared model-specific contract. Rust's request builder embeds this same JSON resource.
public struct ModelReasoningCapabilities: Decodable, Sendable {
    public let model: String
    public let levels: [String]
    public let defaultLevel: String
    public let legacyAliases: [String: String]

    private struct Catalog: Decodable { let models: [ModelReasoningCapabilities] }
    private static let catalog: [ModelReasoningCapabilities] = {
        guard let url = Bundle.module.url(forResource: "ModelReasoningCapabilities", withExtension: "json"),
              let data = try? Data(contentsOf: url),
              let catalog = try? JSONDecoder().decode(Catalog.self, from: data) else {
            preconditionFailure("Missing or invalid model reasoning capability resource")
        }
        return catalog.models
    }()

    public static func forModel(_ name: String) -> ModelReasoningCapabilities? {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return catalog.first { capability in
            if name == capability.model { return true }
            guard name.hasPrefix(capability.model + "-") else { return false }
            let suffix = name.dropFirst(capability.model.count + 1)
            return suffix.count == 10 && suffix.allSatisfy { $0.isNumber || $0 == "-" }
        }
    }

    public func normalized(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased(),
              !value.isEmpty, value != "auto" else { return nil }
        if levels.contains(value) { return value }
        return legacyAliases[value]
    }

    public static func requestEffort(model: String, selected: String?) -> String? {
        if let capability = forModel(model) { return capability.normalized(selected) }
        let value = selected?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return value == "auto" || value == "" ? nil : value
    }
}
