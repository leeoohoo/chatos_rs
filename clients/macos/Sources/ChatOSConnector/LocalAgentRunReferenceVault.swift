import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation

actor LocalAgentRunReferenceVault {
    enum DocumentCreateFailure: Error {
        case empty
        case tooLarge
        case tooMany
        case runTooLarge
        case storage
    }

    enum DocumentReservationResult: Sendable {
        case success([ProjectAgentMessageAttachmentDraft])
        case invalid(index: Int)
        case integrityChanged(index: Int)
    }

    enum SendReceiptLookup: Sendable {
        case missing
        case match(AgentToolOutcome)
        case callIDConflict
    }

    struct MessageAuthority: Sendable {
        let roomID: String
        let messageID: String
    }

    struct TodoAuthority: Sendable {
        let todoID: String
        let agentID: String
        let teamRoomID: String
    }

    struct AssigneeAuthority: Sendable {
        let agentID: String
        let teamRoomID: String
    }

    struct AttachmentAuthority: Sendable {
        let roomID: String
        let messageID: String
        let attachmentID: String
    }

    struct TeamAssetAuthority: Sendable {
        let assetID: String
        let teamRoomID: String
        let revision: Int
    }

    private struct DocumentAuthority: Sendable {
        let localFileURL: URL
        let name: String
        let title: String
        let size: Int
        let sha256: String
        var reservedByCallID: String?
        var consumed: Bool
    }

    private struct SendReceipt: Sendable {
        let signature: String
        let outcome: AgentToolOutcome
    }

    private var conversations: [String: String] = [:]
    private var messages: [String: MessageAuthority] = [:]
    private var todos: [String: TodoAuthority] = [:]
    private var teams: [String: String] = [:]
    private var agents: [String: String] = [:]
    private var assignees: [String: AssigneeAuthority] = [:]
    private var plugins: [String: LocalAgentTodoPluginOption] = [:]
    private var attachments: [String: AttachmentAuthority] = [:]
    private var teamAssets: [String: TeamAssetAuthority] = [:]
    private var documents: [String: DocumentAuthority] = [:]
    private var createdDocumentBytes = 0
    private var sendReceipts: [String: SendReceipt] = [:]
    private let documentDraftDirectoryURL: URL
    private let communicationPolicy: AgentCommunicationPolicy

    init(
        documentDraftDirectoryURL: URL,
        communicationPolicy: AgentCommunicationPolicy = .standard
    ) {
        self.documentDraftDirectoryURL = documentDraftDirectoryURL
        self.communicationPolicy = communicationPolicy
    }

    deinit {
        try? FileManager.default.removeItem(at: documentDraftDirectoryURL)
    }

    func conversationReference(roomID: String) -> String {
        if let existing = conversations.first(where: { $0.value == roomID })?.key {
            return existing
        }
        let reference = "conversation_\(UUID().uuidString.lowercased())"
        conversations[reference] = roomID
        return reference
    }

    func messageReference(roomID: String, messageID: String) -> String {
        if let existing = messages.first(where: {
            $0.value.roomID == roomID && $0.value.messageID == messageID
        })?.key { return existing }
        let reference = "message_\(UUID().uuidString.lowercased())"
        messages[reference] = .init(roomID: roomID, messageID: messageID)
        return reference
    }

    func messageAuthority(reference: String) -> MessageAuthority? { messages[reference] }
    func roomID(conversationReference: String) -> String? {
        conversations[conversationReference]
    }

    func todoReference(todoID: String, agentID: String, teamRoomID: String) -> String {
        if let existing = todos.first(where: { $0.value.todoID == todoID })?.key {
            return existing
        }
        let reference = "todo_\(UUID().uuidString.lowercased())"
        todos[reference] = .init(
            todoID: todoID,
            agentID: agentID,
            teamRoomID: teamRoomID
        )
        return reference
    }

    func todoAuthority(reference: String) -> TodoAuthority? { todos[reference] }

    func teamReference(teamID: String) -> String {
        if let existing = teams.first(where: { $0.value == teamID })?.key { return existing }
        let reference = "team_\(UUID().uuidString.lowercased())"
        teams[reference] = teamID
        return reference
    }

    func teamID(reference: String) -> String? { teams[reference] }

    func agentReference(agentID: String) -> String {
        if let existing = agents.first(where: { $0.value == agentID })?.key { return existing }
        let reference = "agent_\(UUID().uuidString.lowercased())"
        agents[reference] = agentID
        return reference
    }

    func agentID(reference: String) -> String? { agents[reference] }

    func assigneeReference(agentID: String, teamRoomID: String) -> String {
        if let existing = assignees.first(where: {
            $0.value.agentID == agentID && $0.value.teamRoomID == teamRoomID
        })?.key { return existing }
        let reference = "assignee_\(UUID().uuidString.lowercased())"
        assignees[reference] = .init(agentID: agentID, teamRoomID: teamRoomID)
        return reference
    }

    func assigneeAuthority(reference: String) -> AssigneeAuthority? { assignees[reference] }

    func pluginReference(option: LocalAgentTodoPluginOption) -> String {
        if let existing = plugins.first(where: { $0.value.pluginID == option.pluginID })?.key {
            return existing
        }
        let reference = "plugin_\(UUID().uuidString.lowercased())"
        plugins[reference] = option
        return reference
    }

    func plugin(reference: String) -> LocalAgentTodoPluginOption? { plugins[reference] }

    func attachmentReference(roomID: String, messageID: String, attachmentID: String) -> String {
        if let existing = attachments.first(where: {
            $0.value.roomID == roomID
                && $0.value.messageID == messageID
                && $0.value.attachmentID == attachmentID
        })?.key { return existing }
        let reference = "attachment_\(UUID().uuidString.lowercased())"
        attachments[reference] = .init(
            roomID: roomID,
            messageID: messageID,
            attachmentID: attachmentID
        )
        return reference
    }

    func attachmentAuthority(reference: String) -> AttachmentAuthority? {
        attachments[reference]
    }

    func teamAssetReference(assetID: String, teamRoomID: String, revision: Int) -> String {
        if let existing = teamAssets.first(where: {
            $0.value.assetID == assetID
                && $0.value.teamRoomID == teamRoomID
                && $0.value.revision == revision
        })?.key { return existing }
        let reference = "team_asset_\(UUID().uuidString.lowercased())"
        teamAssets[reference] = .init(
            assetID: assetID,
            teamRoomID: teamRoomID,
            revision: revision
        )
        return reference
    }

    func teamAssetAuthority(reference: String) -> TeamAssetAuthority? {
        teamAssets[reference]
    }

    func createDocument(name: String, title: String, data: Data) throws -> (
        reference: String,
        size: Int,
        sha256: String
    ) {
        guard !data.isEmpty else { throw DocumentCreateFailure.empty }
        guard data.count <= communicationPolicy.maximumDocumentBytes else {
            throw DocumentCreateFailure.tooLarge
        }
        guard documents.count < communicationPolicy.maximumDocumentsPerRun else {
            throw DocumentCreateFailure.tooMany
        }
        guard createdDocumentBytes + data.count <= communicationPolicy.maximumDocumentBytesPerRun else {
            throw DocumentCreateFailure.runTooLarge
        }
        let localFileURL = documentDraftDirectoryURL.appendingPathComponent(
            UUID().uuidString.lowercased(),
            isDirectory: false
        )
        do {
            try data.write(to: localFileURL, options: .atomic)
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o600],
                ofItemAtPath: localFileURL.path
            )
        } catch {
            try? FileManager.default.removeItem(at: localFileURL)
            throw DocumentCreateFailure.storage
        }
        let sha256 = Self.sha256(data)
        let reference = "document_\(UUID().uuidString.lowercased())"
        documents[reference] = .init(
            localFileURL: localFileURL,
            name: name,
            title: title,
            size: data.count,
            sha256: sha256,
            reservedByCallID: nil,
            consumed: false
        )
        createdDocumentBytes += data.count
        return (reference, data.count, sha256)
    }

    func reserveDocuments(
        references requestedReferences: [String],
        callID: String
    ) -> DocumentReservationResult {
        var drafts: [ProjectAgentMessageAttachmentDraft] = []
        for (index, reference) in requestedReferences.enumerated() {
            guard let authority = documents[reference],
                  !authority.consumed,
                  authority.reservedByCallID == nil || authority.reservedByCallID == callID else {
                return .invalid(index: index)
            }
            guard let data = try? Data(contentsOf: authority.localFileURL, options: [.mappedIfSafe]),
                  data.count == authority.size,
                  Self.sha256(data) == authority.sha256 else {
                return .integrityChanged(index: index)
            }
            drafts.append(.init(
                name: authority.name,
                mimeType: "text/markdown; charset=utf-8",
                kind: .file,
                origin: .file,
                data: data
            ))
        }
        for reference in requestedReferences {
            documents[reference]?.reservedByCallID = callID
        }
        return .success(drafts)
    }

    func releaseDocuments(references requestedReferences: [String], callID: String) {
        for reference in requestedReferences where documents[reference]?.reservedByCallID == callID {
            documents[reference]?.reservedByCallID = nil
        }
    }

    func consumeDocuments(references requestedReferences: [String], callID: String) {
        for reference in requestedReferences where documents[reference]?.reservedByCallID == callID {
            guard var authority = documents[reference] else { continue }
            authority.reservedByCallID = nil
            authority.consumed = true
            documents[reference] = authority
            try? FileManager.default.removeItem(at: authority.localFileURL)
        }
    }

    func sendReceipt(callID: String, signature: String) -> SendReceiptLookup {
        guard let receipt = sendReceipts[callID] else { return .missing }
        guard receipt.signature == signature else { return .callIDConflict }
        return .match(receipt.outcome)
    }

    func recordSendReceipt(callID: String, signature: String, outcome: AgentToolOutcome) {
        guard sendReceipts[callID] == nil else { return }
        sendReceipts[callID] = .init(signature: signature, outcome: outcome)
    }

    private static func sha256(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }
}
