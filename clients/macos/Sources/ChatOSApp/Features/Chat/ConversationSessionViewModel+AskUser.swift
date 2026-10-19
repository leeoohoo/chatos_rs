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
        askUserPromptRefreshGeneration &+= 1
        let generation = askUserPromptRefreshGeneration
        do {
            let prompts = try await askUserPromptUseCase.fetchPrompts(sessionID: sessionID)
            guard generation == askUserPromptRefreshGeneration else { return }
            replaceAskUserPrompts(prompts)
        } catch {
            guard generation == askUserPromptRefreshGeneration else { return }
            historyError = error.localizedDescription
        }
    }

    func submitAskUserPrompt(_ prompt: AskUserPrompt, submission: AskUserSubmission) {
        guard let askUserPromptUseCase,
              ConversationAskUserPromptUseCase.canSubmit(
                prompt,
                submittingPromptIDs: submittingAskUserPromptIDs
              ) else { return }
        askUserPromptRefreshGeneration &+= 1
        submittingAskUserPromptIDs.insert(prompt.id)
        askUserPromptErrors[prompt.id] = nil
        Task {
            do {
                let updated = try await askUserPromptUseCase.submit(
                    promptID: prompt.id,
                    sessionID: sessionID,
                    submission: submission
                )
                upsertAskUserPrompt(updated)
                refreshLatestSilently()
            } catch {
                askUserPromptErrors[prompt.id] = error.localizedDescription
                await refreshAskUserPrompts()
            }
            submittingAskUserPromptIDs.remove(prompt.id)
        }
    }

    func cancelAskUserPrompt(_ prompt: AskUserPrompt) {
        guard let askUserPromptUseCase,
              ConversationAskUserPromptUseCase.canCancel(
                prompt,
                submittingPromptIDs: submittingAskUserPromptIDs
              ) else { return }
        askUserPromptRefreshGeneration &+= 1
        submittingAskUserPromptIDs.insert(prompt.id)
        askUserPromptErrors[prompt.id] = nil
        Task {
            do {
                let updated = try await askUserPromptUseCase.cancel(
                    promptID: prompt.id,
                    sessionID: sessionID
                )
                upsertAskUserPrompt(updated)
                refreshLatestSilently()
            } catch {
                askUserPromptErrors[prompt.id] = error.localizedDescription
                await refreshAskUserPrompts()
            }
            submittingAskUserPromptIDs.remove(prompt.id)
        }
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
