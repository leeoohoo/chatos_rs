import Foundation

public struct AskUserPromptFormState: Sendable, Equatable {
    public var values: [String: String]
    public var selection: Set<String>

    public init(prompt: AskUserPrompt) {
        self.values = Dictionary(
            uniqueKeysWithValues: prompt.fields.map { ($0.key, $0.defaultValue) }
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
        return AskUserSubmission(values: values, selection: choiceSelection)
    }

    public func isValid(for prompt: AskUserPrompt) -> Bool {
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
        for field in fields where field.isSecret {
            values[field.key] = ""
        }
    }
}
