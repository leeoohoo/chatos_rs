import Foundation

struct ConversationFocusRequest: Sendable, Equatable {
    var id = UUID()
    var turnID: String?
    var promptID: String?
    var taskID: String?
    var runID: String?
}

struct ConversationFocusRefreshToken: Sendable, Equatable {
    let requestID: UUID
}

struct ConversationFocusRefreshState: Sendable, Equatable {
    private(set) var activeToken: ConversationFocusRefreshToken?

    mutating func begin() -> ConversationFocusRefreshToken {
        let token = ConversationFocusRefreshToken(requestID: UUID())
        activeToken = token
        return token
    }

    func accepts(_ token: ConversationFocusRefreshToken) -> Bool {
        activeToken == token
    }

    @discardableResult
    mutating func finish(_ token: ConversationFocusRefreshToken) -> Bool {
        guard accepts(token) else { return false }
        activeToken = nil
        return true
    }
}
