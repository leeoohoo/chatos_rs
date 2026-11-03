import Foundation

public struct AskUserPromptFormState: Sendable, Equatable {
    public var values: [String: String]
    public var selection: Set<String>

    public init(prompt: AskUserPrompt) {
        self.values = Dictionary(
            uniqueKeysWithValues: prompt.fields
                .filter { !$0.isSensitiveCredentialField }
                .map { ($0.key, $0.defaultValue) }
        )
        self.selection = Set(prompt.choice?.defaultSelection ?? [])
    }

    public func submission(for prompt: AskUserPrompt) -> AskUserSubmission {
        let choiceSelection: AskUserSelection?
        if let choice = prompt.choice {
            let ordered = choice.options.map(\.value).filter(selection.contains)
            choiceSelection = choice.allowsMultiple
                ? .multiple(ordered)
                : .single(ordered.first ?? "")
        } else {
            choiceSelection = nil
        }
        let safeKeys = Set(prompt.fields.lazy.filter {
            !$0.isSensitiveCredentialField
        }.map(\.key))
        return AskUserSubmission(
            values: values.filter { safeKeys.contains($0.key) },
            selection: choiceSelection
        )
    }

    public func isValid(for prompt: AskUserPrompt) -> Bool {
        guard !prompt.fields.contains(where: \.isSensitiveCredentialField) else {
            return false
        }
        for field in prompt.fields where field.isRequired {
            if values[field.key, default: ""]
                .trimmingCharacters(in: .whitespacesAndNewlines)
                .isEmpty {
                return false
            }
        }
        if let choice = prompt.choice {
            let count = selection.count
            if count < choice.minimumSelectionCount
                || count > choice.maximumSelectionCount {
                return false
            }
        }
        return true
    }

    public mutating func toggleSelection(
        _ value: String,
        in choice: AskUserChoice
    ) {
        if !choice.allowsMultiple {
            selection = [value]
        } else if selection.contains(value) {
            selection.remove(value)
        } else if selection.count < choice.maximumSelectionCount {
            selection.insert(value)
        }
    }

    public mutating func clearSecretValues(fields: [AskUserField]) {
        for field in fields where field.isSensitiveCredentialField {
            values[field.key] = ""
        }
    }
}
