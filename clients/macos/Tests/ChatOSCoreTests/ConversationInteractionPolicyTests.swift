import ChatOSCore
import Foundation
import Testing

struct ConversationInteractionPolicyTests {
    @Test
    func emptyOrAlreadySendingDraftIsIgnored() {
        #expect(ConversationSendPlanner.plan(
            draft: "  ",
            attachments: [],
            explicitCapabilitySelection: nil,
            turns: [],
            isSending: false
        ) == .ignore)
        #expect(ConversationSendPlanner.plan(
            draft: "hello",
            attachments: [],
            explicitCapabilitySelection: nil,
            turns: [],
            isSending: true
        ) == .ignore)
    }

    @Test
    func activeTurnReceivesGuidanceWithoutExplicitCapability() {
        let plan = ConversationSendPlanner.plan(
            draft: "  continue  ",
            attachments: [],
            explicitCapabilitySelection: nil,
            turns: [turn(id: "active", status: .streaming)],
            isSending: false
        )

        guard case let .guidance(submission, turnID) = plan else {
            Issue.record("Expected guidance plan")
            return
        }
        #expect(turnID == "active")
        #expect(submission.content == "continue")
    }

    @Test
    func explicitCapabilityCannotReplaceActiveTurn() {
        let plan = ConversationSendPlanner.plan(
            draft: "create a document",
            attachments: [],
            explicitCapabilitySelection: .plugin(
                key: "documents",
                componentKey: "document-mcp",
                displayName: "Document"
            ),
            turns: [turn(id: "active", status: .streaming)],
            isSending: false
        )

        #expect(plan == .blocked(.activeTurnRequiresCompletion))
    }

    @Test
    func terminalConversationCreatesNewTurnAndKeepsCapability() {
        let capability = ConversationExplicitCapabilitySelection.plugin(
            key: "documents",
            componentKey: "document-mcp",
            displayName: "Document"
        )
        let plan = ConversationSendPlanner.plan(
            draft: "create a document",
            attachments: [],
            explicitCapabilitySelection: capability,
            turns: [turn(id: "done", status: .completed)],
            isSending: false
        )

        guard case let .newTurn(submission) = plan else {
            Issue.record("Expected new Turn plan")
            return
        }
        #expect(submission.explicitCapabilitySelection == capability)
    }

    @Test
    func realtimePolicyPrioritizesAskUserAndRefreshesTerminalHistory() {
        let askUser = AskUserPromptRealtimeUpdate(
            promptID: "prompt-1",
            sessionID: "session-1",
            turnID: "turn-1",
            action: "created",
            status: .pending
        )
        #expect(ConversationRealtimePolicy.action(for: signal(
            kind: .failed,
            askUserPromptUpdate: askUser
        )) == .refreshAskUserPrompts)
        #expect(ConversationRealtimePolicy.action(for: signal(kind: .completed)) == .refreshHistory)
        #expect(ConversationRealtimePolicy.action(for: signal(kind: .updated)) == .ignore)
    }

    @Test
    func failedRealtimeUsesTrimmedDetailOrFallback() {
        #expect(ConversationRealtimePolicy.action(for: signal(
            kind: .failed,
            detail: "  model failed  "
        )) == .failure(message: "model failed"))
        #expect(ConversationRealtimePolicy.action(for: signal(
            kind: .failed,
            detail: "   "
        )) == .failure(message: ConversationRealtimePolicy.defaultFailureMessage))
    }

    private func turn(id: String, status: TurnStatus) -> ConversationTurn {
        ConversationTurn(
            id: id,
            sessionID: "session-1",
            sequence: 1,
            revision: 1,
            userMessage: ChatMessage(
                id: "message-\(id)",
                role: .user,
                text: id,
                createdAt: Date(timeIntervalSince1970: 1)
            ),
            status: status,
            startedAt: Date(timeIntervalSince1970: 1)
        )
    }

    private func signal(
        kind: ConversationRealtimeKind,
        detail: String? = nil,
        askUserPromptUpdate: AskUserPromptRealtimeUpdate? = nil
    ) -> ConversationRealtimeSignal {
        ConversationRealtimeSignal(
            eventID: UUID().uuidString,
            eventSequence: 1,
            sessionID: "session-1",
            turnID: "turn-1",
            kind: kind,
            eventName: kind.rawValue,
            timestamp: "2026-10-19T00:00:00Z",
            askUserPromptUpdate: askUserPromptUpdate,
            processUpdate: detail.map {
                ConversationRealtimeProcessUpdate(
                    id: "process-1",
                    title: "Run",
                    detail: $0,
                    status: "failed",
                    timestamp: "2026-10-19T00:00:00Z"
                )
            }
        )
    }
}
