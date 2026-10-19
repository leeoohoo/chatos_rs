import ChatOSCore
import Foundation
import Testing

struct ConversationAskUserPromptUseCaseTests {
    @Test
    func fetchUsesFixedConversationLimit() async throws {
        let service = AskUserPromptServiceStub()
        let useCase = ConversationAskUserPromptUseCase(service: service)

        _ = try await useCase.fetchPrompts(sessionID: "session-1")

        #expect(await service.fetches == [
            AskUserFetch(sessionID: "session-1", limit: 100),
        ])
    }

    @Test
    func submitAndCancelForwardTypedCommands() async throws {
        let service = AskUserPromptServiceStub()
        let useCase = ConversationAskUserPromptUseCase(service: service)
        let submission = AskUserSubmission(
            values: ["name": "ChatOS"],
            selection: .single("continue")
        )

        _ = try await useCase.submit(
            promptID: "prompt-1",
            sessionID: "session-1",
            submission: submission
        )
        _ = try await useCase.cancel(
            promptID: "prompt-2",
            sessionID: "session-1"
        )

        #expect(await service.submissions == [
            AskUserSubmit(
                promptID: "prompt-1",
                sessionID: "session-1",
                submission: submission
            ),
        ])
        #expect(await service.cancellations == [
            AskUserCancel(promptID: "prompt-2", sessionID: "session-1"),
        ])
    }

    @Test
    func promptProjectionFiltersAndOrdersByCreatedUpdatedThenID() {
        let prompts = [
            prompt(id: "later", turnID: "turn-1", createdAt: 30),
            prompt(id: "b", turnID: "turn-1", updatedAt: 10),
            prompt(id: "a", turnID: "turn-1", updatedAt: 10),
            prompt(id: "other", turnID: "turn-2", createdAt: 5),
        ]

        let projected = ConversationAskUserPromptUseCase.prompts(
            for: "turn-1",
            in: prompts
        )

        #expect(projected.map(\.id) == ["a", "b", "later"])
    }

    @Test
    func unattachedProjectionKeepsOnlyPendingPromptsForUnloadedTurns() {
        let prompts = [
            prompt(id: "loaded", turnID: "turn-1", createdAt: 1),
            prompt(id: "completed", turnID: "turn-2", status: .ok, createdAt: 2),
            prompt(id: "pending", turnID: "turn-3", createdAt: 3),
        ]

        let projected = ConversationAskUserPromptUseCase.unattachedPendingPrompts(
            in: prompts,
            loadedTurnIDs: ["turn-1"]
        )

        #expect(projected.map(\.id) == ["pending"])
    }

    @Test
    func submissionAndCancellationEligibilityPreservePendingRules() {
        let pending = prompt(id: "pending", turnID: "turn-1")
        let notCancelable = prompt(
            id: "not-cancelable",
            turnID: "turn-1",
            allowsCancel: false
        )
        let completed = prompt(
            id: "completed",
            turnID: "turn-1",
            status: .ok
        )

        #expect(ConversationAskUserPromptUseCase.canSubmit(
            pending,
            submittingPromptIDs: []
        ))
        #expect(!ConversationAskUserPromptUseCase.canSubmit(
            pending,
            submittingPromptIDs: [pending.id]
        ))
        #expect(ConversationAskUserPromptUseCase.canCancel(
            pending,
            submittingPromptIDs: []
        ))
        #expect(!ConversationAskUserPromptUseCase.canCancel(
            notCancelable,
            submittingPromptIDs: []
        ))
        #expect(!ConversationAskUserPromptUseCase.canCancel(
            completed,
            submittingPromptIDs: []
        ))
    }

    @Test
    func upsertReplacesInPlaceOrAppendsWithoutReordering() {
        let first = prompt(id: "first", turnID: "turn-1")
        let second = prompt(id: "second", turnID: "turn-2")
        var updated = first
        updated.status = .ok

        let replaced = ConversationAskUserPromptUseCase.upserting(
            updated,
            in: [first, second]
        )
        let appended = ConversationAskUserPromptUseCase.upserting(
            prompt(id: "third", turnID: "turn-3"),
            in: replaced
        )

        #expect(replaced.map(\.id) == ["first", "second"])
        #expect(replaced.first?.status == .ok)
        #expect(appended.map(\.id) == ["first", "second", "third"])
    }

    private func prompt(
        id: String,
        turnID: String,
        status: AskUserPromptStatus = .pending,
        allowsCancel: Bool = true,
        createdAt: TimeInterval? = nil,
        updatedAt: TimeInterval? = nil
    ) -> AskUserPrompt {
        AskUserPrompt(
            id: id,
            sessionID: "session-1",
            turnID: turnID,
            kind: "choice",
            status: status,
            title: id,
            message: id,
            allowsCancel: allowsCancel,
            createdAt: createdAt.map(Date.init(timeIntervalSince1970:)),
            updatedAt: updatedAt.map(Date.init(timeIntervalSince1970:))
        )
    }
}

private struct AskUserFetch: Sendable, Equatable {
    var sessionID: String
    var limit: Int
}

private struct AskUserSubmit: Sendable, Equatable {
    var promptID: String
    var sessionID: String
    var submission: AskUserSubmission
}

private struct AskUserCancel: Sendable, Equatable {
    var promptID: String
    var sessionID: String
}

private actor AskUserPromptServiceStub: AskUserPromptServicing {
    private(set) var fetches: [AskUserFetch] = []
    private(set) var submissions: [AskUserSubmit] = []
    private(set) var cancellations: [AskUserCancel] = []

    func fetchPrompts(sessionID: String, limit: Int) async throws -> [AskUserPrompt] {
        fetches.append(AskUserFetch(sessionID: sessionID, limit: limit))
        return []
    }

    func submit(
        promptID: String,
        sessionID: String,
        submission: AskUserSubmission
    ) async throws -> AskUserPrompt {
        submissions.append(AskUserSubmit(
            promptID: promptID,
            sessionID: sessionID,
            submission: submission
        ))
        return prompt(id: promptID, sessionID: sessionID, status: .ok)
    }

    func cancel(promptID: String, sessionID: String) async throws -> AskUserPrompt {
        cancellations.append(AskUserCancel(promptID: promptID, sessionID: sessionID))
        return prompt(id: promptID, sessionID: sessionID, status: .canceled)
    }

    private func prompt(
        id: String,
        sessionID: String,
        status: AskUserPromptStatus
    ) -> AskUserPrompt {
        AskUserPrompt(
            id: id,
            sessionID: sessionID,
            turnID: "turn-1",
            kind: "choice",
            status: status,
            title: id,
            message: id,
            allowsCancel: true
        )
    }
}
