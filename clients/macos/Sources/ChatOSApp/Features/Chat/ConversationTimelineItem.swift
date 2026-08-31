import ChatOSCore
import CoreGraphics

enum ConversationTimelineItem: Identifiable {
    case user(turn: ConversationTurn, isFirst: Bool)
    case reply(turn: ConversationTurn, reply: ConversationAssistantReply)
    case prompt(AskUserPrompt)

    var id: String {
        switch self {
        case let .user(turn, _):
            turn.id
        case let .reply(turn, reply):
            "turn-\(turn.id)-reply-\(reply.id)"
        case let .prompt(prompt):
            "ask-user-\(prompt.id)"
        }
    }

    var spacingBefore: CGFloat {
        switch self {
        case let .user(_, isFirst):
            isFirst ? 0 : 22
        case .reply, .prompt:
            14
        }
    }

    static func build(
        turns: [ConversationTurn],
        promptsByTurnID: [String: [AskUserPrompt]],
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
            for prompt in promptsByTurnID[turn.id] ?? [] {
                items.append(.prompt(prompt))
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
