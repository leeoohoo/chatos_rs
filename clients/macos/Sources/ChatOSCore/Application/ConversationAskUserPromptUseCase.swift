import Foundation

public struct ConversationAskUserPromptUseCase: Sendable {
    public static let fetchLimit = 100

    private let service: any AskUserPromptServicing

    public init(service: any AskUserPromptServicing) {
        self.service = service
    }

    public func fetchPrompts(sessionID: String) async throws -> [AskUserPrompt] {
        try await service.fetchPrompts(
            sessionID: sessionID,
            limit: Self.fetchLimit
        )
    }

    public func submit(
        promptID: String,
        sessionID: String,
        submission: AskUserSubmission
    ) async throws -> AskUserPrompt {
        try await service.submit(
            promptID: promptID,
            sessionID: sessionID,
            submission: submission
        )
    }

    public func cancel(
        promptID: String,
        sessionID: String
    ) async throws -> AskUserPrompt {
        try await service.cancel(promptID: promptID, sessionID: sessionID)
    }

    public static func prompts(
        for turnID: String,
        in prompts: [AskUserPrompt]
    ) -> [AskUserPrompt] {
        prompts
            .filter { $0.turnID == turnID }
            .sorted(by: isOrderedBefore)
    }

    public static func unattachedPendingPrompts(
        in prompts: [AskUserPrompt],
        loadedTurnIDs: Set<String>
    ) -> [AskUserPrompt] {
        prompts
            .filter { $0.status.isPending && !loadedTurnIDs.contains($0.turnID) }
            .sorted(by: isOrderedBefore)
    }

    public static func canSubmit(
        _ prompt: AskUserPrompt,
        submittingPromptIDs: Set<String>
    ) -> Bool {
        prompt.status.isPending && !submittingPromptIDs.contains(prompt.id)
    }

    public static func canCancel(
        _ prompt: AskUserPrompt,
        submittingPromptIDs: Set<String>
    ) -> Bool {
        prompt.status.isPending
            && prompt.allowsCancel
            && !submittingPromptIDs.contains(prompt.id)
    }

    public static func upserting(
        _ prompt: AskUserPrompt,
        in prompts: [AskUserPrompt]
    ) -> [AskUserPrompt] {
        var result = prompts
        if let index = result.firstIndex(where: { $0.id == prompt.id }) {
            result[index] = prompt
        } else {
            result.append(prompt)
        }
        return result
    }

    private static func isOrderedBefore(_ lhs: AskUserPrompt, _ rhs: AskUserPrompt) -> Bool {
        let left = lhs.createdAt ?? lhs.updatedAt ?? .distantPast
        let right = rhs.createdAt ?? rhs.updatedAt ?? .distantPast
        if left != right { return left < right }
        return lhs.id < rhs.id
    }
}
