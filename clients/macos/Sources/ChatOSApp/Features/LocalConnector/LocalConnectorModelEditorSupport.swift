import ChatOSCore
import Foundation

struct LocalConnectorTaskModelDraft: Equatable {
    var usage: String
    var thinking: String
    var temperature: String
    var maxOutputTokens: String
    var enabled: Bool

    init(model: LocalConnectorModelConfig) {
        usage = model.taskUsageScenario ?? ""
        thinking = model.taskThinkingLevel ?? ""
        temperature = model.temperature.map { String($0) } ?? ""
        maxOutputTokens = model.maxOutputTokens.map { String($0) } ?? ""
        enabled = model.enabled
    }

    func validatedUpdate(modelName: String) throws -> LocalConnectorModelConfigUpdate {
        let temperatureValue: Double?
        if temperature.trimmed.isEmpty {
            temperatureValue = nil
        } else if let parsed = Double(temperature), (0...2).contains(parsed) {
            temperatureValue = parsed
        } else {
            throw LocalConnectorModelEditorError.invalidTemperature(modelName)
        }

        let tokenValue: Int?
        if maxOutputTokens.trimmed.isEmpty {
            tokenValue = nil
        } else if let parsed = Int(maxOutputTokens), parsed > 0 {
            tokenValue = parsed
        } else {
            throw LocalConnectorModelEditorError.invalidMaxTokens(modelName)
        }

        return .init(
            taskUsageScenario: usage.trimmed.nilIfEmpty,
            taskThinkingLevel: thinking.trimmed.nilIfEmpty,
            temperature: temperatureValue,
            maxOutputTokens: tokenValue,
            enabled: enabled
        )
    }
}

enum LocalConnectorModelEditorError: LocalizedError {
    case invalidTemperature(String)
    case invalidMaxTokens(String)

    var errorDescription: String? {
        switch self {
        case let .invalidTemperature(name):
            "\(name) 的 Temperature 必须留空或填写 0 到 2。"
        case let .invalidMaxTokens(name):
            "\(name) 的 Max Tokens 必须留空或填写大于 0 的整数。"
        }
    }
}

struct LocalConnectorThinkingOption: Identifiable, Equatable {
    var id: String { value.isEmpty ? "default" : value }
    var value: String
    var label: String
}

enum LocalConnectorThinkingOptions {
    static func options(provider: String?) -> [LocalConnectorThinkingOption] {
        switch normalized(provider) {
        case "deepseek":
            return values([("", "默认"), ("none", "关闭"), ("high", "high"), ("max", "max")])
        case "kimi", "kimik2", "moonshot":
            return values([("", "默认"), ("auto", "auto"), ("none", "关闭")])
        case "glm", "zhipu", "zai":
            return values([
                ("", "默认"), ("none", "none"), ("low", "low"),
                ("medium", "medium"), ("high", "high"), ("xhigh", "xhigh"),
            ])
        default:
            return values([
                ("", "默认"), ("none", "none"), ("minimal", "minimal"),
                ("low", "low"), ("medium", "medium"), ("high", "high"),
                ("xhigh", "xhigh"),
            ])
        }
    }

    static func normalizedValue(_ value: String?, provider: String?) -> String {
        let candidate = value?.trimmed ?? ""
        return options(provider: provider).contains(where: { $0.value == candidate })
            ? candidate
            : ""
    }

    private static func normalized(_ provider: String?) -> String {
        (provider ?? "gpt").trimmed.lowercased().replacingOccurrences(of: "-", with: "_")
    }

    private static func values(_ pairs: [(String, String)]) -> [LocalConnectorThinkingOption] {
        pairs.map { .init(value: $0.0, label: $0.1) }
    }
}

extension String {
    fileprivate var trimmed: String {
        trimmingCharacters(in: .whitespacesAndNewlines)
    }

    fileprivate var nilIfEmpty: String? {
        isEmpty ? nil : self
    }
}
