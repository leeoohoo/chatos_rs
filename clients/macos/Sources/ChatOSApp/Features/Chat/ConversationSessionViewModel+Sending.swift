import ChatOSCore
import Foundation

private enum ConversationSendPresentationError: LocalizedError {
    case runtimeUnavailable

    var errorDescription: String? {
        switch self {
        case .runtimeUnavailable:
            "本地任务组件连接已中断，请重新发送。"
        }
    }
}

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
        let token = beginSending(submission)
        let sessionID = sessionID
        sendTask = Task { [weak self] in
            do {
                _ = try await useCase.sendGuidance(
                    sessionID: sessionID,
                    turnID: turnID,
                    submission: submission
                )
                guard !Task.isCancelled,
                      let self,
                      self.sendLifecycleState.accepts(token) else { return }
                self.refreshLatestSilently()
                self.finishSending(token)
            } catch ConversationCommandError.guidanceTargetInactive {
                guard !Task.isCancelled,
                      let self,
                      self.sendLifecycleState.accepts(token) else { return }
                self.startNewTurn(submission, using: useCase, token: token)
            } catch {
                guard !Task.isCancelled,
                      let self,
                      self.sendLifecycleState.accepts(token) else { return }
                self.restoreDraft(submission, error: error)
                self.finishSending(token)
            }
        }
    }

    private func sendNewTurn(
        _ submission: ConversationDraftSubmission,
        using useCase: ConversationSendUseCase
    ) {
        let token = beginSending(submission)
        startNewTurn(submission, using: useCase, token: token)
    }

    private func startNewTurn(
        _ submission: ConversationDraftSubmission,
        using useCase: ConversationSendUseCase,
        token: ConversationSendToken
    ) {
        guard sendLifecycleState.accepts(token) else { return }
        let turn = useCase.makeOptimisticTurn(
            sessionID: sessionID,
            submission: submission,
            existingTurns: turns
        )
        activeSendOptimisticTurnID = turn.id
        selectedTurnID = turn.id
        timelineObservationState.invalidate()
        let sessionID = sessionID
        let reasoningEnabled = reasoningEnabled

        sendTask = Task { [weak self] in
            let stagedSnapshot = await useCase.stageOptimisticTurn(turn)
            guard !Task.isCancelled,
                  self?.presentOptimisticSnapshot(stagedSnapshot, token: token) == true else {
                _ = await useCase.rollbackOptimisticTurn(
                    sessionID: sessionID,
                    turnID: turn.id
                )
                return
            }

            do {
                _ = try await useCase.sendNewTurn(
                    sessionID: sessionID,
                    turnID: turn.id,
                    submission: submission,
                    reasoningEnabled: reasoningEnabled
                )
                guard !Task.isCancelled,
                      let self,
                      self.sendLifecycleState.accepts(token) else { return }
                self.refreshLatestSilently()
                self.finishSending(token)
            } catch {
                let rollbackSnapshot = await useCase.rollbackOptimisticTurn(
                    sessionID: sessionID,
                    turnID: turn.id
                )
                guard !Task.isCancelled,
                      let self,
                      self.sendLifecycleState.accepts(token) else { return }
                self.applyHistorySnapshot(rollbackSnapshot)
                self.restoreDraft(submission, error: error)
                self.finishSending(token)
            }
        }
    }

    private func beginSending(
        _ submission: ConversationDraftSubmission
    ) -> ConversationSendToken {
        sendTask?.cancel()
        let token = sendLifecycleState.begin()
        activeSendSubmission = submission
        activeSendOptimisticTurnID = nil
        draft = ""
        attachments = []
        attachmentError = nil
        selectedCapability = nil
        sendError = nil
        isSending = true
        return token
    }

    private func presentOptimisticSnapshot(
        _ snapshot: ConversationHistorySnapshot,
        token: ConversationSendToken
    ) -> Bool {
        guard sendLifecycleState.accepts(token) else { return false }
        applyHistorySnapshot(snapshot)
        return true
    }

    private func finishSending(_ token: ConversationSendToken) {
        guard sendLifecycleState.finish(token) else { return }
        sendTask = nil
        activeSendSubmission = nil
        activeSendOptimisticTurnID = nil
        isSending = false
    }

    func cancelActiveSendForRuntimeUnavailable() {
        guard let submission = activeSendSubmission else {
            sendLifecycleState.reset()
            sendTask?.cancel()
            sendTask = nil
            if sendRecoveryTask == nil {
                isSending = false
            }
            return
        }

        let optimisticTurnID = activeSendOptimisticTurnID
        sendLifecycleState.reset()
        sendTask?.cancel()
        sendTask = nil
        activeSendSubmission = nil
        activeSendOptimisticTurnID = nil
        restoreDraft(submission, error: ConversationSendPresentationError.runtimeUnavailable)

        guard let optimisticTurnID, let sendUseCase else {
            isSending = false
            return
        }

        let sessionID = sessionID
        sendRecoveryTask?.cancel()
        sendRecoveryTask = Task { [weak self] in
            let snapshot = await sendUseCase.rollbackOptimisticTurn(
                sessionID: sessionID,
                turnID: optimisticTurnID
            )
            guard !Task.isCancelled, let self else { return }
            self.applyHistorySnapshot(snapshot)
            self.sendRecoveryTask = nil
            self.isSending = false
        }
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
