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
        guard let sendUseCase else {
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
                using: sendUseCase
            )
        case let .newTurn(submission):
            sendNewTurn(submission, using: sendUseCase)
        }
    }

    private func sendGuidance(
        _ text: String,
        attachments: [ConversationAttachmentDraft],
        turnID: String,
        using useCase: ConversationSendUseCase
    ) {
        let submission = ConversationDraftSubmission(
            content: text,
            attachments: attachments,
            explicitCapabilitySelection: nil
        )
        beginSending()
        Task {
            do {
                _ = try await useCase.sendGuidance(
                    sessionID: sessionID,
                    turnID: turnID,
                    submission: submission
                )
                refreshLatestSilently()
            } catch ConversationCommandError.guidanceTargetInactive {
                sendNewTurn(submission, using: useCase)
                return
            } catch {
                restoreDraft(submission, error: error)
            }
            isSending = false
        }
    }

    private func sendNewTurn(
        _ submission: ConversationDraftSubmission,
        using useCase: ConversationSendUseCase
    ) {
        let turn = useCase.makeOptimisticTurn(
            sessionID: sessionID,
            submission: submission,
            existingTurns: turns
        )
        beginSending()
        selectedTurnID = turn.id
        timelineObservationState.invalidate()

        Task {
            applyHistorySnapshot(await useCase.stageOptimisticTurn(turn))

            do {
                _ = try await useCase.sendNewTurn(
                    sessionID: sessionID,
                    turnID: turn.id,
                    submission: submission,
                    reasoningEnabled: reasoningEnabled
                )
                refreshLatestSilently()
            } catch {
                applyHistorySnapshot(
                    await useCase.rollbackOptimisticTurn(
                        sessionID: sessionID,
                        turnID: turn.id
                    )
                )
                restoreDraft(submission, error: error)
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
        _ submission: ConversationDraftSubmission,
        error: Error
    ) {
        let recovery = ConversationSendUseCase.recoverComposer(
            currentDraft: draft,
            currentAttachments: attachments,
            currentExplicitCapabilitySelection: selectedCapability,
            failedSubmission: submission
        )
        draft = recovery.draft
        attachments = recovery.attachments
        selectedCapability = recovery.explicitCapabilitySelection
        sendError = error.localizedDescription
    }
}
