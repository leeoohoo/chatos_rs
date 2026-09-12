import ChatOSCore
import Foundation

enum ConversationHistoryMapper {
    static func map(
        _ response: CompactHistoryResponseDTO,
        sessionID: String,
        requestGeneration: Int64
    ) -> HistoryPage {
        let assistantLookup = AssistantLookup(messages: response.items)
        let userMessages = response.items.filter { $0.role == "user" }
        let turns = userMessages.enumerated().map { index, message in
            mapTurn(
                user: message,
                fallbackSequence: Int64(index + 1),
                sessionID: sessionID,
                assistantLookup: assistantLookup
            )
        }

        return HistoryPage(
            turns: turns,
            olderCursor: response.hasMore ? response.nextBefore : nil,
            hasOlder: response.hasMore,
            snapshotRevision: response.snapshotRevision ?? turns.map(\.revision).max() ?? 0,
            requestGeneration: requestGeneration
        )
    }

    private static func mapTurn(
        user: SessionMessageDTO,
        fallbackSequence: Int64,
        sessionID: String,
        assistantLookup: AssistantLookup
    ) -> ConversationTurn {
        let turnID = user.resolvedTurnID
        let assistant = assistantLookup.finalAssistant(for: user, turnID: turnID)
        let assistantReplies = assistantLookup.replies(for: user, turnID: turnID)
        let startedAt = DateParser.parse(user.createdAt) ?? .distantPast
        let completedAt = assistantReplies.last.flatMap {
            DateParser.parse($0.updatedAt ?? $0.createdAt)
        }
        let revision = ([user.resolvedRevision] + assistantReplies.map(\.resolvedRevision)).max() ?? 1
        let processCount = user.metadata.value(at: "historyProcess", "processMessageCount")?.intValue ?? 0
        let status = turnStatus(user: user, assistant: assistant)

        return ConversationTurn(
            id: turnID,
            sessionID: sessionID,
            sequence: user.sequenceNumber ?? fallbackSequence,
            revision: revision,
            userMessage: user.domainMessage(role: .user, fallbackDate: startedAt),
            processEvents: processEvents(
                count: processCount,
                turnID: turnID,
                status: status
            ),
            finalAssistantMessage: assistant?.domainMessage(role: .assistant, fallbackDate: completedAt ?? startedAt),
            assistantReplies: assistantReplies.map {
                ConversationAssistantReply(
                    message: $0.domainMessage(role: .assistant, fallbackDate: completedAt ?? startedAt)
                )
            },
            status: status,
            startedAt: startedAt,
            completedAt: completedAt
        )
    }

    private static func processEvents(
        count: Int,
        turnID: String,
        status: TurnStatus
    ) -> [TurnProcessEvent] {
        guard count > 0 else { return [] }
        return [
            TurnProcessEvent(
                id: "history-process-\(turnID)",
                title: "包含 \(count) 条过程记录",
                detail: "查看这一轮对话的推理、工具调用和中间结果。",
                status: status
            ),
        ]
    }

    private static func turnStatus(
        user: SessionMessageDTO,
        assistant: SessionMessageDTO?
    ) -> TurnStatus {
        let status = (assistant?.status ?? user.status ?? "").lowercased()
        if status == "failed" || status == "error" { return .failed }
        if status == "cancelled" || status == "canceled" { return .cancelled }
        if status == "completed" || status == "succeeded" || status == "success" {
            return .completed
        }
        return assistant == nil ? .streaming : .completed
    }
}

private struct AssistantLookup {
    private var byID: [String: SessionMessageDTO] = [:]
    private var finalsByUserMessageID: [String: SessionMessageDTO] = [:]
    private var finalsByTurnID: [String: SessionMessageDTO] = [:]

    init(messages: [SessionMessageDTO]) {
        for message in messages where message.role == "assistant" {
            byID[message.id] = message
            if let userID = message.metadata.value(at: "historyFinalForUserMessageId")?.stringValue {
                finalsByUserMessageID[userID] = message
            }
            if let turnID = message.metadata.value(at: "historyFinalForTurnId")?.stringValue {
                finalsByTurnID[turnID] = message
            }
        }
    }

    func finalAssistant(for user: SessionMessageDTO, turnID: String) -> SessionMessageDTO? {
        if let assistantID = user.metadata.value(at: "historyProcess", "finalAssistantMessageId")?.stringValue,
           let assistant = byID[assistantID] {
            return assistant
        }
        return finalsByUserMessageID[user.id] ?? finalsByTurnID[turnID]
    }

    func replies(for user: SessionMessageDTO, turnID: String) -> [SessionMessageDTO] {
        finalAssistant(for: user, turnID: turnID).map { [$0] } ?? []
    }
}
