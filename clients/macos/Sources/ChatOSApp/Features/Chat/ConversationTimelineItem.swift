import ChatOSCore
import CoreGraphics
import Foundation

enum ConversationTimelineItem: Identifiable {
    case user(turn: ConversationTurn, isFirst: Bool)
    case additionalUser(turn: ConversationTurn, message: ChatMessage)
    case reply(turn: ConversationTurn, reply: ConversationAssistantReply)
    case prompt(AskUserPrompt)

    var id: String {
        switch self {
        case let .user(turn, _):
            turn.id
        case let .additionalUser(turn, message):
            "turn-\(turn.id)-user-\(message.id)"
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
        case .additionalUser, .reply, .prompt:
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
                $0 += 1 + $1.additionalUserMessages.count + $1.assistantReplies.count
                    + (promptsByTurnID[$1.id]?.count ?? 0)
            }
        )

        for (index, turn) in turns.enumerated() {
            items.append(.user(turn: turn, isFirst: index == 0))
            // Keep the reply presentation contract (handoff before callbacks),
            // but insert guidance using durable ordinals, falling back to timestamps
            // for older history. Ordinals disambiguate writes in the same millisecond.
            var additionalUsers = turn.additionalUserMessages.enumerated().sorted {
                if let lhs = $0.element.storageOrdinal, let rhs = $1.element.storageOrdinal,
                   lhs != rhs { return lhs < rhs }
                if $0.element.createdAt == $1.element.createdAt { return $0.offset < $1.offset }
                return $0.element.createdAt < $1.element.createdAt
            }.map(\.element).makeIterator()
            var nextUser = additionalUsers.next()
            for reply in replies(for: turn) {
                while let message = nextUser, precedesReply(message, reply.message) {
                    items.append(.additionalUser(turn: turn, message: message))
                    nextUser = additionalUsers.next()
                }
                items.append(.reply(turn: turn, reply: reply))
            }
            while let message = nextUser {
                items.append(.additionalUser(turn: turn, message: message))
                nextUser = additionalUsers.next()
            }
            for prompt in promptsByTurnID[turn.id] ?? [] {
                items.append(.prompt(prompt))
            }
        }
        items.append(contentsOf: unattachedPrompts.map(Self.prompt))
        return items
    }

    private static func precedesReply(_ user: ChatMessage, _ reply: ChatMessage) -> Bool {
        if let userOrdinal = user.storageOrdinal, let replyOrdinal = reply.storageOrdinal {
            return userOrdinal <= replyOrdinal
        }
        return user.createdAt <= reply.createdAt
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
