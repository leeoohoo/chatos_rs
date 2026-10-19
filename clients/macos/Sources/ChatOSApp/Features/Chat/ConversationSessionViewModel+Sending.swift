import ChatOSCore
import Foundation

extension ConversationSessionViewModel {
    func sendDraft() {
        let plan = ConversationSendPlanner.plan(
            draft: draft,
            attachments: attachments,
            explicitCapabilitySelection: selectedCapability,
            turns: turns,
            isSending: isSending
        )
        if case .ignore = plan { return }
        guard let commandService else {
            sendError = "聊天发送服务尚未连接。"
            return
        }
        switch plan {
        case .ignore:
            return
        case let .blocked(reason):
            sendError = reason.message
        case let .guidance(submission, turnID):
            sendGuidance(
                submission.content,
                attachments: submission.attachments,
                turnID: turnID,
                using: commandService
            )
        case let .newTurn(submission):
            sendNewTurn(
                submission.content,
                attachments: submission.attachments,
                explicitCapabilitySelection: submission.explicitCapabilitySelection,
                using: commandService
            )
        }
    }

    private func sendGuidance(
        _ text: String,
        attachments: [ConversationAttachmentDraft],
        turnID: String,
        using service: any ConversationCommandServicing
    ) {
        beginSending()
        Task {
            do {
                _ = try await service.sendGuidance(
                    ConversationSendCommand(
                        sessionID: sessionID,
                        turnID: turnID,
                        content: text,
                        attachments: attachments
                    )
                )
                refreshLatestSilently()
            } catch ConversationCommandError.guidanceTargetInactive {
                sendNewTurn(text, attachments: attachments, using: service)
                return
            } catch {
                restoreDraft(text, attachments: attachments, error: error)
            }
            isSending = false
        }
    }

    private func sendNewTurn(
        _ text: String,
        attachments: [ConversationAttachmentDraft],
        explicitCapabilitySelection: ConversationExplicitCapabilitySelection? = nil,
        using service: any ConversationCommandServicing
    ) {
        let turn = makeOptimisticTurn(text, attachments: attachments)
        beginSending()
        selectedTurnID = turn.id
        timelineObservationState.invalidate()

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
                        content: text,
                        attachments: attachments,
                        reasoningEnabled: reasoningEnabled,
                        explicitCapabilitySelection: explicitCapabilitySelection
                    )
                )
                refreshLatestSilently()
            } catch {
                await historyStore.discardOptimisticTurn(sessionID: sessionID, turnID: turn.id)
                await refreshSnapshot()
                restoreDraft(
                    text,
                    attachments: attachments,
                    explicitCapabilitySelection: explicitCapabilitySelection,
                    error: error
                )
            }
            isSending = false
        }
    }

    private func beginSending() {
        draft = ""
        attachments = []
        attachmentError = nil
        selectedCapability = nil
        sendError = nil
        isSending = true
    }

    private func restoreDraft(
        _ text: String,
        attachments: [ConversationAttachmentDraft],
        explicitCapabilitySelection: ConversationExplicitCapabilitySelection? = nil,
        error: Error
    ) {
        if draft.isEmpty { draft = text }
        let currentIDs = Set(self.attachments.map(\.id))
        self.attachments.insert(
            contentsOf: attachments.filter { !currentIDs.contains($0.id) },
            at: 0
        )
        if selectedCapability == nil {
            selectedCapability = explicitCapabilitySelection
        }
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
