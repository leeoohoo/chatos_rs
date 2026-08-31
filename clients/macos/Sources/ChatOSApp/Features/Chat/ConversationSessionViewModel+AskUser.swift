import ChatOSCore
import Foundation

extension ConversationSessionViewModel {
    func prompts(for turnID: String) -> [AskUserPrompt] {
        askUserPrompts
            .filter { $0.turnID == turnID }
            .sorted(by: Self.askUserPromptOrder)
    }

    var unattachedPendingPrompts: [AskUserPrompt] {
        let loadedTurnIDs = Set(turns.map(\.id))
        return askUserPrompts
            .filter { $0.status.isPending && !loadedTurnIDs.contains($0.turnID) }
            .sorted(by: Self.askUserPromptOrder)
    }

    func isSubmitting(promptID: String) -> Bool {
        submittingAskUserPromptIDs.contains(promptID)
    }

    func refreshAskUserPrompts() async {
        guard let askUserPromptService else { return }
        do {
            askUserPrompts = try await askUserPromptService.fetchPrompts(
                sessionID: sessionID,
                limit: 100
            )
        } catch {
            historyError = error.localizedDescription
        }
    }

    func submitAskUserPrompt(_ prompt: AskUserPrompt, submission: AskUserSubmission) {
        guard let askUserPromptService,
              prompt.status.isPending,
              !submittingAskUserPromptIDs.contains(prompt.id) else { return }
        submittingAskUserPromptIDs.insert(prompt.id)
        askUserPromptErrors[prompt.id] = nil
        Task {
            do {
                let updated = try await askUserPromptService.submit(
                    promptID: prompt.id,
                    sessionID: sessionID,
                    submission: submission
                )
                upsertAskUserPrompt(updated)
                refreshLatest()
            } catch {
                askUserPromptErrors[prompt.id] = error.localizedDescription
                await refreshAskUserPrompts()
            }
            submittingAskUserPromptIDs.remove(prompt.id)
        }
    }

    func cancelAskUserPrompt(_ prompt: AskUserPrompt) {
        guard let askUserPromptService,
              prompt.status.isPending,
              prompt.allowsCancel,
              !submittingAskUserPromptIDs.contains(prompt.id) else { return }
        submittingAskUserPromptIDs.insert(prompt.id)
        askUserPromptErrors[prompt.id] = nil
        Task {
            do {
                let updated = try await askUserPromptService.cancel(
                    promptID: prompt.id,
                    sessionID: sessionID
                )
                upsertAskUserPrompt(updated)
                refreshLatest()
            } catch {
                askUserPromptErrors[prompt.id] = error.localizedDescription
                await refreshAskUserPrompts()
            }
            submittingAskUserPromptIDs.remove(prompt.id)
        }
    }

    private func upsertAskUserPrompt(_ prompt: AskUserPrompt) {
        if let index = askUserPrompts.firstIndex(where: { $0.id == prompt.id }) {
            askUserPrompts[index] = prompt
        } else {
            askUserPrompts.append(prompt)
        }
    }

    private static func askUserPromptOrder(_ lhs: AskUserPrompt, _ rhs: AskUserPrompt) -> Bool {
        let left = lhs.createdAt ?? lhs.updatedAt ?? .distantPast
        let right = rhs.createdAt ?? rhs.updatedAt ?? .distantPast
        if left != right { return left < right }
        return lhs.id < rhs.id
    }
}
