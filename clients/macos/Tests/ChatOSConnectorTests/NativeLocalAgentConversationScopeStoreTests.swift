// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
@testable import ChatOSConnector
import Foundation
import Testing

struct NativeLocalAgentConversationScopeStoreTests {
    @Test("binds server conversation relations to one authenticated account")
    func freezesTrustedRoutes() async throws {
        let store = NativeLocalAgentConversationScopeStore()
        let conversation = WorkspaceConversation(
            id: "conversation-1",
            title: "Design",
            projectID: "project-1",
            contactID: "contact-1",
            contactAgentID: "agent-1",
            messageCount: 0,
            updatedAt: .now,
            isArchived: false
        )

        await store.update(conversations: [conversation], accountID: "user-1")
        let scope = try await store.scope(conversationID: "conversation-1")

        #expect(scope.accountID == "user-1")
        #expect(scope.projectID == "project-1")
        #expect(scope.contactAgentID == "agent-1")
        await store.deactivate()
        await #expect(throws: NativeLocalAgentConversationScopeError.inactive) {
            _ = try await store.scope(conversationID: "conversation-1")
        }
    }
}
