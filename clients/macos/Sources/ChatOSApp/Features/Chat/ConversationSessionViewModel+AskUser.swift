import ChatOSCore
import Foundation

extension ConversationSessionViewModel {
    func prompts(for turnID: String) -> [AskUserPrompt] {
        ConversationAskUserPromptUseCase.prompts(
            for: turnID,
            in: askUserPrompts
        )
    }

    var unattachedPendingPrompts: [AskUserPrompt] {
        ConversationAskUserPromptUseCase.unattachedPendingPrompts(
            in: askUserPrompts,
            loadedTurnIDs: Set(turns.map(\.id))
        )
    }

    func isSubmitting(promptID: String) -> Bool {
        submittingAskUserPromptIDs.contains(promptID)
    }

    func refreshAskUserPrompts() async {
        guard isLocalAgentRuntimeReady, let askUserPromptUseCase else { return }
        askUserPromptRefreshTask?.cancel()
        let token = askUserPromptLifecycleState.beginRefresh()
        let sessionID = sessionID
        let refreshTask = Task {
            try await askUserPromptUseCase.fetchPrompts(sessionID: sessionID)
        }
        askUserPromptRefreshTask = refreshTask
        do {
            let prompts = try await withTaskCancellationHandler {
                try await refreshTask.value
            } onCancel: {
                refreshTask.cancel()
            }
            try Task.checkCancellation()
            guard isLocalAgentRuntimeReady,
                  askUserPromptLifecycleState.finishRefresh(token) else { return }
            askUserPromptRefreshTask = nil
            replaceAskUserPrompts(prompts)
        } catch is CancellationError {
            guard askUserPromptLifecycleState.accepts(token) else { return }
            _ = askUserPromptLifecycleState.finishRefresh(token)
            askUserPromptRefreshTask = nil
        } catch {
            guard isLocalAgentRuntimeReady,
                  askUserPromptLifecycleState.finishRefresh(token) else { return }
            askUserPromptRefreshTask = nil
            historyError = error.localizedDescription
        }
    }

    func submitAskUserPrompt(_ prompt: AskUserPrompt, submission: AskUserSubmission) {
        guard let askUserPromptUseCase,
              ConversationAskUserPromptUseCase.canSubmit(
                prompt,
                submittingPromptIDs: submittingAskUserPromptIDs
              ) else { return }
        let sessionID = sessionID
        startAskUserPromptAction(promptID: prompt.id) {
            try await askUserPromptUseCase.submit(
                promptID: prompt.id,
                sessionID: sessionID,
                submission: submission
            )
        }
    }

    func cancelAskUserPrompt(_ prompt: AskUserPrompt) {
        guard let askUserPromptUseCase,
              ConversationAskUserPromptUseCase.canCancel(
                prompt,
                submittingPromptIDs: submittingAskUserPromptIDs
              ) else { return }
        let sessionID = sessionID
        startAskUserPromptAction(promptID: prompt.id) {
            try await askUserPromptUseCase.cancel(
                promptID: prompt.id,
                sessionID: sessionID
            )
        }
    }

    private func startAskUserPromptAction(
        promptID: String,
        operation: @escaping @Sendable () async throws -> AskUserPrompt
    ) {
        guard let token = askUserPromptLifecycleState.beginAction(promptID: promptID) else {
            return
        }
        askUserPromptRefreshTask?.cancel()
        askUserPromptRefreshTask = nil
        submittingAskUserPromptIDs = askUserPromptLifecycleState.activePromptIDs
        askUserPromptErrors.removeValue(forKey: promptID)
        askUserPromptActionTasks[promptID] = Task { [weak self] in
            var updatedPrompt: AskUserPrompt?
            var failure: Error?
            do {
                updatedPrompt = try await operation()
                try Task.checkCancellation()
            } catch is CancellationError {
                if Task.isCancelled { return }
                failure = CancellationError()
            } catch {
                failure = error
            }
            guard let self,
                  isLocalAgentRuntimeReady,
                  askUserPromptLifecycleState.accepts(token) else { return }
            if let updatedPrompt {
                finishAskUserPromptAction(token)
                upsertAskUserPrompt(updatedPrompt)
                refreshLatestSilently()
            } else if let failure {
                askUserPromptErrors[promptID] = failure.localizedDescription
                await refreshAskUserPrompts()
                guard isLocalAgentRuntimeReady,
                      askUserPromptLifecycleState.accepts(token) else { return }
                finishAskUserPromptAction(token)
            }
        }
    }

    private func finishAskUserPromptAction(
        _ token: ConversationAskUserPromptActionToken
    ) {
        guard askUserPromptLifecycleState.finishAction(token) else { return }
        askUserPromptRefreshTask?.cancel()
        askUserPromptRefreshTask = nil
        askUserPromptActionTasks[token.promptID] = nil
        submittingAskUserPromptIDs = askUserPromptLifecycleState.activePromptIDs
    }

    private func upsertAskUserPrompt(_ prompt: AskUserPrompt) {
        replaceAskUserPrompts(
            ConversationAskUserPromptUseCase.upserting(
                prompt,
                in: askUserPrompts
            )
        )
    }
}
