import ChatOSCore
import Foundation
import Testing

struct ConversationSendUseCaseTests {
    @Test
    func optimisticTurnUsesNextSequenceAndStagesSnapshot() async {
        let store = ConversationHistoryStore()
        let service = ConversationSendCommandServiceStub()
        let useCase = ConversationSendUseCase(
            commandService: service,
            historyStore: store
        )
        let attachment = attachment(id: "attachment-1")
        let submission = ConversationDraftSubmission(
            content: "继续执行",
            attachments: [attachment],
            explicitCapabilitySelection: nil
        )

        let optimistic = useCase.makeOptimisticTurn(
            sessionID: "session-1",
            submission: submission,
            existingTurns: [turn(id: "older", sequence: 7, revision: 1)]
        )
        let snapshot = await useCase.stageOptimisticTurn(optimistic)

        #expect(optimistic.id.hasPrefix("turn_"))
        #expect(optimistic.sessionID == "session-1")
        #expect(optimistic.sequence == 8)
        #expect(optimistic.revision == 0)
        #expect(optimistic.status == .streaming)
        #expect(optimistic.userMessage.id.hasPrefix("optimistic_"))
        #expect(optimistic.userMessage.text == "继续执行")
        #expect(optimistic.userMessage.attachments == [attachment.reference])
        #expect(snapshot.turns == [optimistic])
    }

    @Test
    func newTurnCommandPreservesReasoningAttachmentsAndCapability() async throws {
        let store = ConversationHistoryStore()
        let service = ConversationSendCommandServiceStub()
        let useCase = ConversationSendUseCase(
            commandService: service,
            historyStore: store
        )
        let submission = ConversationDraftSubmission(
            content: "创建文档",
            attachments: [attachment(id: "attachment-1")],
            explicitCapabilitySelection: .plugin(
                key: "document-plugin",
                componentKey: "document-mcp",
                displayName: "Document"
            )
        )

        _ = try await useCase.sendNewTurn(
            sessionID: "session-1",
            turnID: "turn-1",
            submission: submission,
            reasoningEnabled: true
        )

        let command = await service.newTurnCommands.first
        #expect(command?.sessionID == "session-1")
        #expect(command?.turnID == "turn-1")
        #expect(command?.content == submission.content)
        #expect(command?.attachments == submission.attachments)
        #expect(command?.reasoningEnabled == true)
        #expect(command?.explicitCapabilitySelection == submission.explicitCapabilitySelection)
    }

    @Test
    func guidanceCommandTargetsExistingTurnWithoutNewTurnOptions() async throws {
        let store = ConversationHistoryStore()
        let service = ConversationSendCommandServiceStub()
        let useCase = ConversationSendUseCase(
            commandService: service,
            historyStore: store
        )
        let submission = ConversationDraftSubmission(
            content: "补充要求",
            attachments: [attachment(id: "attachment-1")],
            explicitCapabilitySelection: nil
        )

        _ = try await useCase.sendGuidance(
            sessionID: "session-1",
            turnID: "active-turn",
            submission: submission
        )

        let command = await service.guidanceCommands.first
        #expect(command?.turnID == "active-turn")
        #expect(command?.content == submission.content)
        #expect(command?.attachments == submission.attachments)
        #expect(command?.reasoningEnabled == nil)
        #expect(command?.explicitCapabilitySelection == nil)
    }

    @Test
    func rollbackRemovesOnlyStagedOptimisticTurn() async {
        let store = ConversationHistoryStore()
        let service = ConversationSendCommandServiceStub()
        let useCase = ConversationSendUseCase(
            commandService: service,
            historyStore: store
        )
        let persisted = turn(id: "persisted", sequence: 1, revision: 2)
        await store.mergeCachedTurns([persisted], sessionID: "session-1")
        let optimistic = useCase.makeOptimisticTurn(
            sessionID: "session-1",
            submission: ConversationDraftSubmission(
                content: "执行",
                attachments: [],
                explicitCapabilitySelection: nil
            ),
            existingTurns: [persisted]
        )
        _ = await useCase.stageOptimisticTurn(optimistic)

        let snapshot = await useCase.rollbackOptimisticTurn(
            sessionID: "session-1",
            turnID: optimistic.id
        )

        #expect(snapshot.turns == [persisted])
    }

    @Test
    func composerRecoveryKeepsNewDraftDeduplicatesAttachmentsAndRestoresCapability() {
        let failedOnly = attachment(id: "failed-only")
        let duplicate = attachment(id: "duplicate")
        let currentOnly = attachment(id: "current-only")
        let capability = ConversationExplicitCapabilitySelection.plugin(
            key: "document-plugin",
            componentKey: "document-mcp",
            displayName: "Document"
        )
        let recovery = ConversationSendUseCase.recoverComposer(
            currentDraft: "用户后来输入的内容",
            currentAttachments: [duplicate, currentOnly],
            currentExplicitCapabilitySelection: nil,
            failedSubmission: ConversationDraftSubmission(
                content: "发送失败的内容",
                attachments: [failedOnly, duplicate],
                explicitCapabilitySelection: capability
            )
        )

        #expect(recovery.draft == "用户后来输入的内容")
        #expect(recovery.attachments.map(\.id) == ["failed-only", "duplicate", "current-only"])
        #expect(recovery.explicitCapabilitySelection == capability)
    }

    private func attachment(id: String) -> ConversationAttachmentDraft {
        ConversationAttachmentDraft(
            id: id,
            name: "\(id).txt",
            mimeType: "text/plain",
            kind: .file,
            origin: .file,
            data: Data(id.utf8)
        )
    }

    private func turn(
        id: String,
        sequence: Int64,
        revision: Int64
    ) -> ConversationTurn {
        ConversationTurn(
            id: id,
            sessionID: "session-1",
            sequence: sequence,
            revision: revision,
            userMessage: ChatMessage(
                id: "message-\(id)",
                role: .user,
                text: id,
                createdAt: Date(timeIntervalSince1970: TimeInterval(sequence))
            ),
            status: revision == 0 ? .streaming : .completed,
            startedAt: Date(timeIntervalSince1970: TimeInterval(sequence))
        )
    }
}

private actor ConversationSendCommandServiceStub: ConversationCommandServicing {
    private(set) var newTurnCommands: [ConversationSendCommand] = []
    private(set) var guidanceCommands: [ConversationSendCommand] = []

    func sendNewTurn(_ command: ConversationSendCommand) async throws -> ConversationCommandAck {
        newTurnCommands.append(command)
        return ConversationCommandAck(
            accepted: true,
            turnID: command.turnID,
            userMessageID: "message-1"
        )
    }

    func sendGuidance(_ command: ConversationSendCommand) async throws -> ConversationCommandAck {
        guidanceCommands.append(command)
        return ConversationCommandAck(
            accepted: true,
            turnID: command.turnID,
            userMessageID: "message-1"
        )
    }

    func stopTurn(conversationID: String, turnID: String?) async throws {}
}
