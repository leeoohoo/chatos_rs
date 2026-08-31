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
        let taskLookup = mergedTaskLookup(
            user.messageTaskLookup,
            assistant?.messageTaskLookup,
            sessionID: sessionID
        )
        let projectExecutionContext = user.projectExecutionContext ?? assistant?.projectExecutionContext
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
                    message: $0.domainMessage(role: .assistant, fallbackDate: completedAt ?? startedAt),
                    taskCallback: $0.taskRunnerCallbackReference
                )
            },
            messageTaskLookup: taskLookup,
            projectExecutionContext: projectExecutionContext,
            status: status,
            startedAt: startedAt,
            completedAt: completedAt
        )
    }

    private static func mergedTaskLookup(
        _ primary: MessageTaskLookup?,
        _ secondary: MessageTaskLookup?,
        sessionID: String
    ) -> MessageTaskLookup? {
        guard primary != nil || secondary != nil else { return nil }
        return MessageTaskLookup(
            sessionID: sessionID,
            turnID: primary?.turnID ?? secondary?.turnID,
            sourceUserMessageID: primary?.sourceUserMessageID ?? secondary?.sourceUserMessageID
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
        let taskRunnerStatus = user.metadata.value(
            at: "task_runner_async",
            "overall_status"
        )?.stringValue ?? user.metadata.value(
            at: "task_runner_async",
            "confirmation_status"
        )?.stringValue
        let status = (assistant?.status ?? user.status ?? taskRunnerStatus ?? "").lowercased()
        if status == "failed" || status == "error" { return .failed }
        if status == "cancelled" || status == "canceled" { return .cancelled }
        if status == "completed" || status == "succeeded" || status == "success" {
            return .completed
        }
        return assistant == nil ? .streaming : .completed
    }
}

private struct AssistantLookup {
    private struct IndexedMessage: Sendable {
        var index: Int
        var message: SessionMessageDTO
    }

    private var byID: [String: IndexedMessage] = [:]
    private var finalsByUserMessageID: [String: IndexedMessage] = [:]
    private var finalsByTurnID: [String: IndexedMessage] = [:]
    private var callbacksByUserMessageID: [String: [IndexedMessage]] = [:]
    private var callbacksByTurnID: [String: [IndexedMessage]] = [:]

    init(messages: [SessionMessageDTO]) {
        for (index, message) in messages.enumerated()
        where message.role == "assistant" && !message.isCancelledTaskCallback {
            let indexed = IndexedMessage(index: index, message: message)
            byID[message.id] = indexed
            if message.isTaskRunnerCallback {
                if let sourceUserID = message.taskRunnerCallbackReference?.sourceUserMessageID {
                    callbacksByUserMessageID[sourceUserID, default: []].append(indexed)
                }
                if let sourceTurnID = message.taskRunnerCallbackReference?.sourceTurnID {
                    callbacksByTurnID[sourceTurnID, default: []].append(indexed)
                }
                continue
            }
            if let userID = message.metadata.value(at: "historyFinalForUserMessageId")?.stringValue {
                finalsByUserMessageID[userID] = indexed
            }
            if let turnID = message.finalTurnID {
                finalsByTurnID[turnID] = indexed
            }
        }
    }

    func finalAssistant(for user: SessionMessageDTO, turnID: String) -> SessionMessageDTO? {
        if let assistantID = user.metadata.value(at: "historyProcess", "finalAssistantMessageId")?.stringValue,
           let assistant = byID[assistantID]?.message,
           !assistant.isTaskRunnerCallback {
            return assistant
        }
        return finalsByUserMessageID[user.id]?.message ?? finalsByTurnID[turnID]?.message
    }

    func replies(for user: SessionMessageDTO, turnID: String) -> [SessionMessageDTO] {
        var indexed: [IndexedMessage] = []
        if let final = finalAssistant(for: user, turnID: turnID),
           let finalIndexed = byID[final.id] {
            indexed.append(finalIndexed)
        }
        indexed.append(contentsOf: callbacksByUserMessageID[user.id] ?? [])
        indexed.append(contentsOf: callbacksByTurnID[turnID] ?? [])

        var seen = Set<String>()
        return indexed
            .sorted { $0.index < $1.index }
            .compactMap { item in
                guard seen.insert(item.message.id).inserted else { return nil }
                return item.message
            }
    }
}
