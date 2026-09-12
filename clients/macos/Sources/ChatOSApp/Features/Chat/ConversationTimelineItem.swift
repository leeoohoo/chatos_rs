import ChatOSCore
import CoreGraphics

enum ConversationTimelineItem: Identifiable {
    case user(turn: ConversationTurn, isFirst: Bool)
    case reply(turn: ConversationTurn, reply: ConversationAssistantReply)
    case task(LocalAgentTaskState)
    case prompt(AskUserPrompt)
    case toolApproval(LocalAgentToolApprovalRequest)
    case runControl(LocalAgentRunControlState)

    var id: String {
        switch self {
        case let .user(turn, _):
            turn.id
        case let .reply(turn, reply):
            "turn-\(turn.id)-reply-\(reply.id)"
        case let .prompt(prompt):
            "ask-user-\(prompt.id)"
        case let .toolApproval(approval):
            "local-agent-tool-approval-\(approval.invocationID)"
        case let .runControl(control):
            "local-agent-run-control-\(control.runID)"
        case let .task(task):
            "local-agent-task-\(task.id)"
        }
    }

    var spacingBefore: CGFloat {
        switch self {
        case let .user(_, isFirst):
            isFirst ? 0 : 22
        case .reply, .task, .prompt, .toolApproval, .runControl:
            14
        }
    }

    static func build(
        turns: [ConversationTurn],
        promptsByTurnID: [String: [AskUserPrompt]],
        toolApprovalsByTurnID: [String: [LocalAgentToolApprovalRequest]],
        runControlsByTurnID: [String: [LocalAgentRunControlState]],
        tasksByTurnID: [String: [LocalAgentTaskState]],
        unattachedPrompts: [AskUserPrompt]
    ) -> [ConversationTimelineItem] {
        var items: [ConversationTimelineItem] = []
        items.reserveCapacity(
            turns.reduce(into: unattachedPrompts.count) {
                $0 += 1 + $1.assistantReplies.count + (promptsByTurnID[$1.id]?.count ?? 0)
            }
        )

        for (index, turn) in turns.enumerated() {
            items.append(.user(turn: turn, isFirst: index == 0))
            for reply in replies(for: turn) {
                items.append(.reply(turn: turn, reply: reply))
            }
            for task in tasksByTurnID[turn.id] ?? [] {
                items.append(.task(task))
            }
            for prompt in promptsByTurnID[turn.id] ?? [] {
                items.append(.prompt(prompt))
            }
            for approval in toolApprovalsByTurnID[turn.id] ?? [] {
                items.append(.toolApproval(approval))
            }
            for control in runControlsByTurnID[turn.id] ?? [] {
                items.append(.runControl(control))
            }
        }
        items.append(contentsOf: unattachedPrompts.map(Self.prompt))
        return items
    }

    private static func replies(for turn: ConversationTurn) -> [ConversationAssistantReply] {
        if !turn.assistantReplies.isEmpty {
            return turn.assistantReplies
        }
        return turn.finalAssistantMessage.map {
            [ConversationAssistantReply(message: $0)]
        } ?? []
    }
}
