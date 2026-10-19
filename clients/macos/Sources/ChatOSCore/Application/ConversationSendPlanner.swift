import Foundation

public struct ConversationDraftSubmission: Sendable, Equatable {
    public var content: String
    public var attachments: [ConversationAttachmentDraft]
    public var explicitCapabilitySelection: ConversationExplicitCapabilitySelection?

    public init(
        content: String,
        attachments: [ConversationAttachmentDraft],
        explicitCapabilitySelection: ConversationExplicitCapabilitySelection?
    ) {
        self.content = content
        self.attachments = attachments
        self.explicitCapabilitySelection = explicitCapabilitySelection
    }
}

public enum ConversationSendBlockReason: Sendable, Equatable {
    case activeTurnRequiresCompletion

    public var message: String {
        switch self {
        case .activeTurnRequiresCompletion:
            "当前轮次仍在执行。请等待结束或停止后，再用所选能力发起新任务。"
        }
    }
}

public enum ConversationSendPlan: Sendable, Equatable {
    case ignore
    case blocked(ConversationSendBlockReason)
    case guidance(submission: ConversationDraftSubmission, turnID: String)
    case newTurn(ConversationDraftSubmission)
}

/// Application policy for selecting the macOS Conversation send path.
public enum ConversationSendPlanner {
    public static func plan(
        draft: String,
        attachments: [ConversationAttachmentDraft],
        explicitCapabilitySelection: ConversationExplicitCapabilitySelection?,
        turns: [ConversationTurn],
        isSending: Bool
    ) -> ConversationSendPlan {
        let content = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard (!content.isEmpty || !attachments.isEmpty), !isSending else {
            return .ignore
        }

        let submission = ConversationDraftSubmission(
            content: content,
            attachments: attachments,
            explicitCapabilitySelection: explicitCapabilitySelection
        )
        guard let activeTurn = turns.last(where: { $0.status == .streaming }) else {
            return .newTurn(submission)
        }
        guard explicitCapabilitySelection == nil else {
            return .blocked(.activeTurnRequiresCompletion)
        }
        return .guidance(submission: submission, turnID: activeTurn.id)
    }
}
