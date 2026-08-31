import Foundation

public enum AskUserPromptStatus: String, Codable, Sendable, Equatable {
    case pending
    case ok
    case canceled
    case timeout
    case failed

    public var isPending: Bool { self == .pending }
}

public struct AskUserField: Identifiable, Codable, Sendable, Equatable {
    public var id: String { key }
    public var key: String
    public var label: String
    public var description: String?
    public var placeholder: String?
    public var defaultValue: String
    public var isRequired: Bool
    public var isMultiline: Bool
    public var isSecret: Bool

    public init(
        key: String,
        label: String,
        description: String? = nil,
        placeholder: String? = nil,
        defaultValue: String = "",
        isRequired: Bool = false,
        isMultiline: Bool = false,
        isSecret: Bool = false
    ) {
        self.key = key
        self.label = label
        self.description = description
        self.placeholder = placeholder
        self.defaultValue = defaultValue
        self.isRequired = isRequired
        self.isMultiline = isMultiline
        self.isSecret = isSecret
    }
}

public struct AskUserChoiceOption: Identifiable, Codable, Sendable, Equatable {
    public var id: String { value }
    public var value: String
    public var label: String
    public var description: String?

    public init(value: String, label: String, description: String? = nil) {
        self.value = value
        self.label = label
        self.description = description
    }
}

public struct AskUserChoice: Codable, Sendable, Equatable {
    public var allowsMultiple: Bool
    public var options: [AskUserChoiceOption]
    public var defaultSelection: [String]
    public var minimumSelectionCount: Int
    public var maximumSelectionCount: Int

    public init(
        allowsMultiple: Bool,
        options: [AskUserChoiceOption],
        defaultSelection: [String] = [],
        minimumSelectionCount: Int = 0,
        maximumSelectionCount: Int? = nil
    ) {
        self.allowsMultiple = allowsMultiple
        self.options = options
        self.defaultSelection = defaultSelection
        self.minimumSelectionCount = max(0, minimumSelectionCount)
        self.maximumSelectionCount = max(
            self.minimumSelectionCount,
            maximumSelectionCount ?? (allowsMultiple ? options.count : 1)
        )
    }
}

public struct AskUserPrompt: Identifiable, Codable, Sendable, Equatable {
    public var id: String
    public var sessionID: String
    public var turnID: String
    public var toolCallID: String?
    public var kind: String
    public var status: AskUserPromptStatus
    public var title: String
    public var message: String
    public var allowsCancel: Bool
    public var timeoutMilliseconds: Int64?
    public var fields: [AskUserField]
    public var choice: AskUserChoice?
    public var createdAt: Date?
    public var updatedAt: Date?

    public init(
        id: String,
        sessionID: String,
        turnID: String,
        toolCallID: String? = nil,
        kind: String,
        status: AskUserPromptStatus,
        title: String,
        message: String,
        allowsCancel: Bool,
        timeoutMilliseconds: Int64? = nil,
        fields: [AskUserField] = [],
        choice: AskUserChoice? = nil,
        createdAt: Date? = nil,
        updatedAt: Date? = nil
    ) {
        self.id = id
        self.sessionID = sessionID
        self.turnID = turnID
        self.toolCallID = toolCallID
        self.kind = kind
        self.status = status
        self.title = title
        self.message = message
        self.allowsCancel = allowsCancel
        self.timeoutMilliseconds = timeoutMilliseconds
        self.fields = fields
        self.choice = choice
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

public enum AskUserSelection: Sendable, Equatable {
    case single(String)
    case multiple([String])
}

public struct AskUserSubmission: Sendable, Equatable {
    public var values: [String: String]
    public var selection: AskUserSelection?

    public init(
        values: [String: String] = [:],
        selection: AskUserSelection? = nil
    ) {
        self.values = values
        self.selection = selection
    }
}

public protocol AskUserPromptServicing: Sendable {
    func fetchPrompts(sessionID: String, limit: Int) async throws -> [AskUserPrompt]
    func submit(
        promptID: String,
        sessionID: String,
        submission: AskUserSubmission
    ) async throws -> AskUserPrompt
    func cancel(promptID: String, sessionID: String) async throws -> AskUserPrompt
}

public struct AskUserPromptRealtimeUpdate: Sendable, Equatable {
    public var promptID: String
    public var sessionID: String
    public var turnID: String?
    public var action: String
    public var status: AskUserPromptStatus?

    public init(
        promptID: String,
        sessionID: String,
        turnID: String?,
        action: String,
        status: AskUserPromptStatus?
    ) {
        self.promptID = promptID
        self.sessionID = sessionID
        self.turnID = turnID
        self.action = action
        self.status = status
    }
}
