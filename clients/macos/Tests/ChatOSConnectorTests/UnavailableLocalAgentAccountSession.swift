@testable import ChatOSConnector
import ChatOSCore

struct UnavailableLocalAgentAccountSession: NativeLocalAgentAccountSessionAccess {
    func client(accountID _: String) async throws -> NativeLocalAgentIPCClient {
        throw NativeLocalAgentAccountSessionError.inactive
    }

    func activeClient() async throws -> NativeLocalAgentIPCClient {
        throw NativeLocalAgentAccountSessionError.inactive
    }

    func stageAttachments(
        _ attachments: [ConversationAttachmentDraft],
        accountID _: String
    ) async throws -> [LocalAgentAttachmentReference] {
        guard attachments.isEmpty else { throw NativeLocalAgentAccountSessionError.inactive }
        return []
    }

    func discardStagedAttachments(
        _: [LocalAgentAttachmentReference],
        accountID _: String
    ) async {}
}
