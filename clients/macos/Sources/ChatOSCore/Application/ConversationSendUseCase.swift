import Foundation

public struct ConversationComposerRecovery: Sendable, Equatable {
    public var draft: String
    public var attachments: [ConversationAttachmentDraft]
    public var explicitCapabilitySelection: ConversationExplicitCapabilitySelection?

    public init(
        draft: String,
        attachments: [ConversationAttachmentDraft],
        explicitCapabilitySelection: ConversationExplicitCapabilitySelection?
    ) {
        self.draft = draft
        self.attachments = attachments
        self.explicitCapabilitySelection = explicitCapabilitySelection
    }
}

/// Application use case for sending Conversation commands and coordinating optimistic history.
///
/// UI clients retain ownership of presentation state and Task cancellation. This use case owns
/// the ordering and data rules that must remain identical across macOS and Windows clients.
public struct ConversationSendUseCase: Sendable {
    private let commandService: any ConversationCommandServicing
    private let historyStore: any ConversationHistoryStoring

    public init(
        commandService: any ConversationCommandServicing,
        historyStore: any ConversationHistoryStoring
    ) {
        self.commandService = commandService
        self.historyStore = historyStore
    }

    public func makeOptimisticTurn(
        sessionID: String,
        submission: ConversationDraftSubmission,
        existingTurns: [ConversationTurn]
    ) -> ConversationTurn {
        let now = Date()
        return ConversationTurn(
            id: "turn_\(UUID().uuidString.lowercased())",
            sessionID: sessionID,
            sequence: (existingTurns.map(\.sequence).max() ?? 0) + 1,
            revision: 0,
            userMessage: ChatMessage(
                id: "optimistic_\(UUID().uuidString.lowercased())",
                role: .user,
                text: submission.content,
                createdAt: now,
                attachments: submission.attachments.map(\.reference)
            ),
            status: .streaming,
            startedAt: now
        )
    }

    public func stageOptimisticTurn(
        _ turn: ConversationTurn
    ) async -> ConversationHistorySnapshot {
        await historyStore.applyRealtime(
            RealtimeTurnEvent(
                eventID: "optimistic-\(turn.id)",
                eventSequence: turn.sequence,
                turn: turn
            ),
            userIsReadingOlderContent: false
        )
        return await historyStore.snapshot(sessionID: turn.sessionID)
    }

    public func sendNewTurn(
        sessionID: String,
        turnID: String,
        submission: ConversationDraftSubmission,
        reasoningEnabled: Bool
    ) async throws -> ConversationCommandAck {
        try await commandService.sendNewTurn(
            ConversationSendCommand(
                sessionID: sessionID,
                turnID: turnID,
                content: submission.content,
                attachments: submission.attachments,
                reasoningEnabled: reasoningEnabled,
                explicitCapabilitySelection: submission.explicitCapabilitySelection
            )
        )
    }

    public func sendGuidance(
        sessionID: String,
        turnID: String,
        submission: ConversationDraftSubmission
    ) async throws -> ConversationCommandAck {
        try await commandService.sendGuidance(
            ConversationSendCommand(
                sessionID: sessionID,
                turnID: turnID,
                content: submission.content,
                attachments: submission.attachments
            )
        )
    }

    public func rollbackOptimisticTurn(
        sessionID: String,
        turnID: String
    ) async -> ConversationHistorySnapshot {
        await historyStore.discardOptimisticTurn(
            sessionID: sessionID,
            turnID: turnID
        )
        return await historyStore.snapshot(sessionID: sessionID)
    }

    public static func recoverComposer(
        currentDraft: String,
        currentAttachments: [ConversationAttachmentDraft],
        currentExplicitCapabilitySelection: ConversationExplicitCapabilitySelection?,
        failedSubmission: ConversationDraftSubmission
    ) -> ConversationComposerRecovery {
        let currentIDs = Set(currentAttachments.map(\.id))
        let restoredAttachments = failedSubmission.attachments.filter {
            !currentIDs.contains($0.id)
        } + currentAttachments
        return ConversationComposerRecovery(
            draft: currentDraft.isEmpty ? failedSubmission.content : currentDraft,
            attachments: restoredAttachments,
            explicitCapabilitySelection: currentExplicitCapabilitySelection
                ?? failedSubmission.explicitCapabilitySelection
        )
    }
}
