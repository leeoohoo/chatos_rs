import Foundation

public enum LocalAgentRequirementSurveyStatus: String, Codable, Sendable, CaseIterable {
    case pending
    case submitted
}

public enum LocalAgentRequirementSurveyQuestionKind: String, Codable, Sendable, CaseIterable {
    case singleChoice = "single_choice"
    case multipleChoice = "multiple_choice"
    case ranking
}

public struct LocalAgentRequirementSurveyOption: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let label: String

    public init(id: String, label: String) {
        self.id = id
        self.label = label
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(id, field: "requirementSurveyOptionID")
        try AgentGroupChatValidation.text(
            label,
            field: "requirementSurveyOptionLabel",
            maximumLength: 500
        )
    }
}

public struct LocalAgentRequirementSurveyQuestion: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let prompt: String
    public let kind: LocalAgentRequirementSurveyQuestionKind
    public let options: [LocalAgentRequirementSurveyOption]
    public let isRequired: Bool

    public init(
        id: String,
        prompt: String,
        kind: LocalAgentRequirementSurveyQuestionKind,
        options: [LocalAgentRequirementSurveyOption],
        isRequired: Bool = true
    ) {
        self.id = id
        self.prompt = prompt
        self.kind = kind
        self.options = options
        self.isRequired = isRequired
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(id, field: "requirementSurveyQuestionID")
        try AgentGroupChatValidation.text(
            prompt,
            field: "requirementSurveyQuestionPrompt",
            maximumLength: 1_000
        )
        guard (2...12).contains(options.count),
              Set(options.map(\.id)).count == options.count else {
            throw AgentGroupChatError.invalidField("requirementSurveyOptions")
        }
        if kind == .ranking, options.count > 10 {
            throw AgentGroupChatError.invalidField("requirementSurveyOptions")
        }
        for option in options { try option.validate() }
    }
}

public struct LocalAgentRequirementSurveyDraft: Codable, Sendable, Equatable {
    public let title: String
    public let purpose: String
    public let questions: [LocalAgentRequirementSurveyQuestion]

    public init(
        title: String,
        purpose: String,
        questions: [LocalAgentRequirementSurveyQuestion]
    ) {
        self.title = title
        self.purpose = purpose
        self.questions = questions
    }

    public func validate() throws {
        try AgentGroupChatValidation.text(
            title,
            field: "requirementSurveyTitle",
            maximumLength: 240
        )
        try AgentGroupChatValidation.text(
            purpose,
            field: "requirementSurveyPurpose",
            maximumLength: 4_000
        )
        guard (1...12).contains(questions.count),
              Set(questions.map(\.id)).count == questions.count else {
            throw AgentGroupChatError.invalidField("requirementSurveyQuestions")
        }
        for question in questions { try question.validate() }
    }
}

public struct LocalAgentRequirementSurveyAnswer: Codable, Sendable, Equatable, Identifiable {
    public var id: String { questionID }
    public let questionID: String
    public let selectedOptionIDs: [String]

    public init(questionID: String, selectedOptionIDs: [String]) {
        self.questionID = questionID
        self.selectedOptionIDs = selectedOptionIDs
    }
}

public struct LocalAgentRequirementSurveySubmission: Codable, Sendable, Equatable {
    public let answers: [LocalAgentRequirementSurveyAnswer]
    public let notes: String

    public init(answers: [LocalAgentRequirementSurveyAnswer], notes: String = "") {
        self.answers = answers
        self.notes = notes
    }
}

public struct LocalAgentRequirementSurveyExecutionStep: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let title: String
    public let detail: String
    public let owner: String
    public let deliverable: String
    public let acceptanceCriteria: String

    public init(
        id: String,
        title: String,
        detail: String,
        owner: String = "",
        deliverable: String = "",
        acceptanceCriteria: String = ""
    ) {
        self.id = id
        self.title = title
        self.detail = detail
        self.owner = owner
        self.deliverable = deliverable
        self.acceptanceCriteria = acceptanceCriteria
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(id, field: "requirementSurveyStepID")
        try AgentGroupChatValidation.text(
            title,
            field: "requirementSurveyStepTitle",
            maximumLength: 500
        )
        try AgentGroupChatValidation.text(
            detail,
            field: "requirementSurveyStepDetail",
            maximumLength: 8_000
        )
        try AgentGroupChatValidation.optionalText(
            owner,
            field: "requirementSurveyStepOwner",
            maximumLength: 500
        )
        try AgentGroupChatValidation.optionalText(
            deliverable,
            field: "requirementSurveyStepDeliverable",
            maximumLength: 4_000
        )
        try AgentGroupChatValidation.optionalText(
            acceptanceCriteria,
            field: "requirementSurveyStepAcceptanceCriteria",
            maximumLength: 4_000
        )
    }
}

public struct LocalAgentRequirementSurveyResolution: Codable, Sendable, Equatable {
    public let summary: String
    public let solutionMarkdown: String
    public let executionSteps: [LocalAgentRequirementSurveyExecutionStep]
    public let risksAndOpenQuestions: String
    public let relatedMaterials: String

    public init(
        summary: String,
        solutionMarkdown: String,
        executionSteps: [LocalAgentRequirementSurveyExecutionStep],
        risksAndOpenQuestions: String = "",
        relatedMaterials: String = ""
    ) {
        self.summary = summary
        self.solutionMarkdown = solutionMarkdown
        self.executionSteps = executionSteps
        self.risksAndOpenQuestions = risksAndOpenQuestions
        self.relatedMaterials = relatedMaterials
    }

    public func validate() throws {
        try AgentGroupChatValidation.text(
            summary,
            field: "requirementSurveyResolutionSummary",
            maximumLength: 4_000
        )
        try AgentGroupChatValidation.text(
            solutionMarkdown,
            field: "requirementSurveySolution",
            maximumLength: 128_000
        )
        guard (1...50).contains(executionSteps.count),
              Set(executionSteps.map(\.id)).count == executionSteps.count else {
            throw AgentGroupChatError.invalidField("requirementSurveyExecutionSteps")
        }
        for step in executionSteps { try step.validate() }
        try AgentGroupChatValidation.optionalText(
            risksAndOpenQuestions,
            field: "requirementSurveyRisksAndOpenQuestions",
            maximumLength: 32_000
        )
        try AgentGroupChatValidation.optionalText(
            relatedMaterials,
            field: "requirementSurveyRelatedMaterials",
            maximumLength: 32_000
        )
    }
}

/// A reusable Human confirmation form owned directly by a project. The Agent or conversation
/// that created it is provenance only; teams and rooms never define survey ownership.
public struct LocalAgentRequirementSurvey: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let ownerUserID: String
    public let projectID: String
    public let creatorAgentID: String
    public let sourceDeliveryID: String
    public let requestKey: String
    public let draft: LocalAgentRequirementSurveyDraft
    public let status: LocalAgentRequirementSurveyStatus
    public let submission: LocalAgentRequirementSurveySubmission?
    public let resolution: LocalAgentRequirementSurveyResolution?
    public let createdAtUnixMs: Int64
    public let submittedAtUnixMs: Int64?
    public let resolvedAtUnixMs: Int64?

    public init(
        id: String,
        ownerUserID: String,
        projectID: String,
        creatorAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentRequirementSurveyDraft,
        status: LocalAgentRequirementSurveyStatus = .pending,
        submission: LocalAgentRequirementSurveySubmission? = nil,
        resolution: LocalAgentRequirementSurveyResolution? = nil,
        createdAtUnixMs: Int64,
        submittedAtUnixMs: Int64? = nil,
        resolvedAtUnixMs: Int64? = nil
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.projectID = projectID
        self.creatorAgentID = creatorAgentID
        self.sourceDeliveryID = sourceDeliveryID
        self.requestKey = requestKey
        self.draft = draft
        self.status = status
        self.submission = submission
        self.resolution = resolution
        self.createdAtUnixMs = createdAtUnixMs
        self.submittedAtUnixMs = submittedAtUnixMs
        self.resolvedAtUnixMs = resolvedAtUnixMs
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(id, field: "requirementSurveyID")
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(projectID, field: "projectID")
        try AgentGroupChatValidation.identifier(creatorAgentID, field: "creatorAgentID")
        try AgentGroupChatValidation.identifier(sourceDeliveryID, field: "sourceDeliveryID")
        try AgentGroupChatValidation.identifier(requestKey, field: "requestKey")
        try draft.validate()
        guard createdAtUnixMs >= 0 else {
            throw AgentGroupChatError.invalidField("requirementSurveyCreatedAt")
        }
        switch status {
        case .pending:
            guard submission == nil, submittedAtUnixMs == nil,
                  resolution == nil, resolvedAtUnixMs == nil else {
                throw AgentGroupChatError.invalidField("requirementSurveyStatus")
            }
        case .submitted:
            guard let submission, let submittedAtUnixMs,
                  submittedAtUnixMs >= createdAtUnixMs else {
                throw AgentGroupChatError.invalidField("requirementSurveyStatus")
            }
            try Self.validate(submission: submission, questions: draft.questions)
            if let resolution {
                guard let resolvedAtUnixMs, resolvedAtUnixMs >= submittedAtUnixMs else {
                    throw AgentGroupChatError.invalidField("requirementSurveyResolution")
                }
                try resolution.validate()
            } else if resolvedAtUnixMs != nil {
                throw AgentGroupChatError.invalidField("requirementSurveyResolution")
            }
        }
    }

    public static func validate(
        submission: LocalAgentRequirementSurveySubmission,
        questions: [LocalAgentRequirementSurveyQuestion]
    ) throws {
        try AgentGroupChatValidation.optionalText(
            submission.notes,
            field: "requirementSurveyNotes",
            maximumLength: 16_000
        )
        guard Set(submission.answers.map(\.questionID)).count == submission.answers.count else {
            throw AgentGroupChatError.invalidField("requirementSurveyAnswers")
        }
        let answersByQuestion = Dictionary(
            uniqueKeysWithValues: submission.answers.map { ($0.questionID, $0) }
        )
        guard Set(answersByQuestion.keys).isSubset(of: Set(questions.map(\.id))) else {
            throw AgentGroupChatError.invalidField("requirementSurveyAnswers")
        }
        for question in questions {
            let selected = answersByQuestion[question.id]?.selectedOptionIDs ?? []
            guard Set(selected).count == selected.count,
                  Set(selected).isSubset(of: Set(question.options.map(\.id))),
                  !question.isRequired || !selected.isEmpty else {
                throw AgentGroupChatError.invalidField("requirementSurveyAnswers")
            }
            switch question.kind {
            case .singleChoice:
                guard selected.count <= 1 else {
                    throw AgentGroupChatError.invalidField("requirementSurveyAnswers")
                }
            case .multipleChoice:
                break
            case .ranking:
                guard selected.isEmpty || selected.count == question.options.count else {
                    throw AgentGroupChatError.invalidField("requirementSurveyAnswers")
                }
            }
        }
    }
}
