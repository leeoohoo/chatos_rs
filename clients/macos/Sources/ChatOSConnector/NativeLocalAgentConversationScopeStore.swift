// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

public enum NativeLocalAgentConversationScopeError: Error, Equatable, Sendable {
    case inactive
    case unknownConversation
}

extension NativeLocalAgentConversationScopeError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .inactive: "本地 Agent 会话账户尚未激活"
        case .unknownConversation: "本地 Agent 找不到当前会话的可信作用域"
        }
    }
}

public struct NativeLocalAgentConversationScope: Sendable, Equatable {
    public var accountID: String
    public var conversationID: String
    public var projectID: String?
    public var contactAgentID: String?

    public init(
        accountID: String,
        conversationID: String,
        projectID: String?,
        contactAgentID: String?
    ) {
        self.accountID = accountID
        self.conversationID = conversationID
        self.projectID = projectID
        self.contactAgentID = contactAgentID
    }
}

/// Account-scoped routing authority for native Main Chat creation. Models and
/// composer payloads never select or override a project or contact Agent.
public actor NativeLocalAgentConversationScopeStore {
    private var accountID: String?
    private var scopes: [String: NativeLocalAgentConversationScope] = [:]

    public init() {}

    public func activate(accountID: String) {
        guard self.accountID != accountID else { return }
        self.accountID = accountID
        scopes = [:]
    }

    public func update(conversations: [WorkspaceConversation], accountID: String) {
        activate(accountID: accountID)
        var next: [String: NativeLocalAgentConversationScope] = [:]
        for conversation in conversations {
            next[conversation.id] = NativeLocalAgentConversationScope(
                accountID: accountID,
                conversationID: conversation.id,
                projectID: conversation.projectID,
                contactAgentID: conversation.contactAgentID
            )
        }
        scopes = next
    }

    public func deactivate() {
        accountID = nil
        scopes = [:]
    }

    public func scope(conversationID: String) throws -> NativeLocalAgentConversationScope {
        guard accountID != nil else {
            throw NativeLocalAgentConversationScopeError.inactive
        }
        guard let scope = scopes[conversationID] else {
            throw NativeLocalAgentConversationScopeError.unknownConversation
        }
        return scope
    }
}
