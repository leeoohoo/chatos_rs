import ChatOSCore
import Foundation

extension ConversationSessionViewModel {
    func sendDraft() {
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        let outgoingAttachments = attachments
        guard (!text.isEmpty || !outgoingAttachments.isEmpty), !isSending else { return }
        guard let commandService else {
            sendError = "聊天发送服务尚未连接。"
            return
        }

        sendNewTurn(text, attachments: outgoingAttachments, using: commandService)
    }

    private func sendNewTurn(
        _ text: String,
        attachments: [ConversationAttachmentDraft],
        using service: any ConversationCommandServicing
    ) {
        let turn = makeOptimisticTurn(text, attachments: attachments)
        beginSending()
        selectedTurnID = turn.id

        Task {
            await historyStore.applyRealtime(
                RealtimeTurnEvent(
                    eventID: "optimistic-\(turn.id)",
                    eventSequence: turn.sequence,
                    turn: turn
                ),
                userIsReadingOlderContent: false
            )
            await refreshSnapshot()

            do {
                _ = try await service.sendNewTurn(
                    ConversationSendCommand(
                        sessionID: sessionID,
                        turnID: turn.id,
                        messageID: turn.userMessage.id,
                        content: text,
                        attachments: attachments,
                        reasoningEnabled: reasoningEnabled
                    )
                )
            } catch {
                await historyStore.discardOptimisticTurn(sessionID: sessionID, turnID: turn.id)
                await refreshSnapshot()
                restoreDraft(text, attachments: attachments, error: error)
            }
            isSending = false
        }
    }

    private func beginSending() {
        draft = ""
        attachments = []
        attachmentError = nil
        sendError = nil
        isSending = true
    }

    private func restoreDraft(
        _ text: String,
        attachments: [ConversationAttachmentDraft],
        error: Error
    ) {
        if draft.isEmpty { draft = text }
        let currentIDs = Set(self.attachments.map(\.id))
        self.attachments.insert(
            contentsOf: attachments.filter { !currentIDs.contains($0.id) },
            at: 0
        )
        sendError = error.localizedDescription
    }

    private func makeOptimisticTurn(
        _ text: String,
        attachments: [ConversationAttachmentDraft]
    ) -> ConversationTurn {
        let now = Date()
        return ConversationTurn(
            id: "turn_\(UUID().uuidString.lowercased())",
            sessionID: sessionID,
            sequence: (turns.map(\.sequence).max() ?? 0) + 1,
            revision: 0,
            userMessage: ChatMessage(
                id: "optimistic_\(UUID().uuidString.lowercased())",
                role: .user,
                text: text,
                createdAt: now,
                attachments: attachments.map(\.reference)
            ),
            status: .streaming,
            startedAt: now
        )
    }
}
