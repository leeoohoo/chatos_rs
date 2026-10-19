import ChatOSCore
import Foundation

typealias AskUserPromptRefreshRequest = (
    token: ConversationAskUserPromptRefreshToken,
    task: Task<[AskUserPrompt], Error>
)

func awaitAskUserPromptRefresh(
    _ task: Task<[AskUserPrompt], Error>
) async -> Result<[AskUserPrompt], Error> {
    do {
        let prompts = try await withTaskCancellationHandler {
            try await task.value
        } onCancel: {
            task.cancel()
        }
        try Task.checkCancellation()
        return .success(prompts)
    } catch {
        return .failure(error)
    }
}

func awaitAskUserPromptRefreshRequest(
    _ request: AskUserPromptRefreshRequest?
) async -> Result<[AskUserPrompt], Error>? {
    guard let request else { return nil }
    return await awaitAskUserPromptRefresh(request.task)
}

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
        guard let request = beginAskUserPromptRefresh() else { return }
        let result = await awaitAskUserPromptRefresh(request.task)
        finishAskUserPromptRefresh(result, token: request.token)
    }

    func beginAskUserPromptRefresh() -> AskUserPromptRefreshRequest? {
        guard isLocalAgentRuntimeReady, let askUserPromptUseCase else { return nil }
        askUserPromptRefreshTask?.cancel()
        let token = askUserPromptLifecycleState.beginRefresh()
        let sessionID = sessionID
        let refreshTask = Task {
            try await askUserPromptUseCase.fetchPrompts(sessionID: sessionID)
        }
        askUserPromptRefreshTask = refreshTask
        return (token, refreshTask)
    }

    func finishAskUserPromptRefresh(
        _ result: Result<[AskUserPrompt], Error>,
        token: ConversationAskUserPromptRefreshToken
    ) {
        switch result {
        case let .success(prompts):
            guard isLocalAgentRuntimeReady,
                  askUserPromptLifecycleState.finishRefresh(token) else { return }
            askUserPromptRefreshTask = nil
            replaceAskUserPrompts(prompts)
        case .failure(is CancellationError):
            guard askUserPromptLifecycleState.accepts(token) else { return }
            _ = askUserPromptLifecycleState.finishRefresh(token)
            askUserPromptRefreshTask = nil
        case let .failure(error):
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
            if let updatedPrompt {
                guard let self,
                      self.isLocalAgentRuntimeReady,
                      self.askUserPromptLifecycleState.accepts(token) else { return }
                self.finishAskUserPromptAction(token)
                self.upsertAskUserPrompt(updatedPrompt)
                self.refreshLatestSilently()
                return
            }
            guard let failure else { return }

            let refreshRequest: AskUserPromptRefreshRequest
            if let self {
                guard self.isLocalAgentRuntimeReady,
                      self.askUserPromptLifecycleState.accepts(token) else { return }
                self.askUserPromptErrors[promptID] = failure.localizedDescription
                guard let request = self.beginAskUserPromptRefresh() else {
                    self.finishAskUserPromptAction(token)
                    return
                }
                refreshRequest = request
            } else {
                return
            }

            let refreshResult = await awaitAskUserPromptRefresh(refreshRequest.task)
            guard let self,
                  self.isLocalAgentRuntimeReady,
                  self.askUserPromptLifecycleState.accepts(token) else { return }
            if self.askUserPromptLifecycleState.accepts(refreshRequest.token) {
                self.finishAskUserPromptRefresh(
                    refreshResult,
                    token: refreshRequest.token
                )
            }
            guard self.askUserPromptLifecycleState.accepts(token) else { return }
            self.finishAskUserPromptAction(token)
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
