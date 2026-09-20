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

    struct MessageAuthority: Codable, Sendable {
        let roomID: String
        let messageID: String
    }

    struct TodoAuthority: Codable, Sendable {
        let todoID: String
        let agentID: String
        let teamRoomID: String
    }

    struct AssigneeAuthority: Codable, Sendable {
        let agentID: String
        let teamRoomID: String
    }

    struct AttachmentAuthority: Codable, Sendable {
        let roomID: String
        let messageID: String
        let attachmentID: String
    }

    struct TeamAssetAuthority: Codable, Sendable {
        let assetID: String
        let teamRoomID: String
        let revision: Int
    }

    private struct StringAuthority: Codable, Sendable {
        let value: String
    }

    private struct SealedReference<Value: Codable & Sendable>: Codable, Sendable {
        let version: Int
        let kind: String
        let authority: Value
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
    private let referenceKey: SymmetricKey
    private let pluginOptionsByID: [String: LocalAgentTodoPluginOption]

    init(
        documentDraftDirectoryURL: URL,
        runContext: LocalAgentChatRunContext,
        pluginOptions: [LocalAgentTodoPluginOption] = [],
        communicationPolicy: AgentCommunicationPolicy = .standard
    ) {
        self.documentDraftDirectoryURL = documentDraftDirectoryURL
        self.communicationPolicy = communicationPolicy
        self.pluginOptionsByID = pluginOptions.reduce(into: [:]) { result, option in
            result[option.pluginID] = option
        }
        var keyMaterial = Data("chatos-local-agent-run-reference-v1".utf8)
        for value in [
            runContext.ownerUserID,
            runContext.projectID,
            runContext.roomID,
            runContext.agentID,
            runContext.deliveryID,
            runContext.runID,
        ] {
            keyMaterial.append(Data("\u{0}\(value.utf8.count):".utf8))
            keyMaterial.append(Data(value.utf8))
        }
        self.referenceKey = SymmetricKey(data: SHA256.hash(data: keyMaterial))
    }

    deinit {
        try? FileManager.default.removeItem(at: documentDraftDirectoryURL)
    }

    func conversationReference(roomID: String) -> String {
        if let existing = conversations.first(where: { $0.value == roomID })?.key {
            return existing
        }
        let reference = sealedReference(
            prefix: "conversation_",
            kind: "conversation",
            authority: StringAuthority(value: roomID)
        )
        conversations[reference] = roomID
        return reference
    }

    func messageReference(roomID: String, messageID: String) -> String {
        if let existing = messages.first(where: {
            $0.value.roomID == roomID && $0.value.messageID == messageID
        })?.key { return existing }
        let authority = MessageAuthority(roomID: roomID, messageID: messageID)
        let reference = sealedReference(
            prefix: "message_",
            kind: "message",
            authority: authority
        )
        messages[reference] = authority
        return reference
    }

    func messageAuthority(reference: String) -> MessageAuthority? {
        messages[reference] ?? openedReference(
            reference,
            prefix: "message_",
            kind: "message",
            as: MessageAuthority.self
        )
    }
    func roomID(conversationReference: String) -> String? {
        conversations[conversationReference] ?? openedReference(
            conversationReference,
            prefix: "conversation_",
            kind: "conversation",
            as: StringAuthority.self
        )?.value
    }

    func todoReference(todoID: String, agentID: String, teamRoomID: String) -> String {
        if let existing = todos.first(where: { $0.value.todoID == todoID })?.key {
            return existing
        }
        let authority = TodoAuthority(
            todoID: todoID,
            agentID: agentID,
            teamRoomID: teamRoomID
        )
        let reference = sealedReference(prefix: "todo_", kind: "todo", authority: authority)
        todos[reference] = authority
        return reference
    }

    func todoAuthority(reference: String) -> TodoAuthority? {
        todos[reference] ?? openedReference(
            reference,
            prefix: "todo_",
            kind: "todo",
            as: TodoAuthority.self
        )
    }

    func teamReference(teamID: String) -> String {
        if let existing = teams.first(where: { $0.value == teamID })?.key { return existing }
        let reference = sealedReference(
            prefix: "team_",
            kind: "team",
            authority: StringAuthority(value: teamID)
        )
        teams[reference] = teamID
        return reference
    }

    func teamID(reference: String) -> String? {
        teams[reference] ?? openedReference(
            reference,
            prefix: "team_",
            kind: "team",
            as: StringAuthority.self
        )?.value
    }

    func agentReference(agentID: String) -> String {
        if let existing = agents.first(where: { $0.value == agentID })?.key { return existing }
        let reference = sealedReference(
            prefix: "agent_",
            kind: "agent",
            authority: StringAuthority(value: agentID)
        )
        agents[reference] = agentID
        return reference
    }

    func agentID(reference: String) -> String? {
        agents[reference] ?? openedReference(
            reference,
            prefix: "agent_",
            kind: "agent",
            as: StringAuthority.self
        )?.value
    }

    func assigneeReference(agentID: String, teamRoomID: String) -> String {
        if let existing = assignees.first(where: {
            $0.value.agentID == agentID && $0.value.teamRoomID == teamRoomID
        })?.key { return existing }
        let authority = AssigneeAuthority(agentID: agentID, teamRoomID: teamRoomID)
        let reference = sealedReference(
            prefix: "assignee_",
            kind: "assignee",
            authority: authority
        )
        assignees[reference] = authority
        return reference
    }

    func assigneeAuthority(reference: String) -> AssigneeAuthority? {
        assignees[reference] ?? openedReference(
            reference,
            prefix: "assignee_",
            kind: "assignee",
            as: AssigneeAuthority.self
        )
    }

    func pluginReference(option: LocalAgentTodoPluginOption) -> String {
        if let existing = plugins.first(where: { $0.value.pluginID == option.pluginID })?.key {
            return existing
        }
        let reference = sealedReference(
            prefix: "plugin_",
            kind: "plugin",
            authority: StringAuthority(value: option.pluginID)
        )
        plugins[reference] = option
        return reference
    }

    func plugin(reference: String) -> LocalAgentTodoPluginOption? {
        if let option = plugins[reference] { return option }
        guard let pluginID = openedReference(
            reference,
            prefix: "plugin_",
            kind: "plugin",
            as: StringAuthority.self
        )?.value else { return nil }
        return pluginOptionsByID[pluginID]
    }

    func attachmentReference(roomID: String, messageID: String, attachmentID: String) -> String {
        if let existing = attachments.first(where: {
            $0.value.roomID == roomID
                && $0.value.messageID == messageID
                && $0.value.attachmentID == attachmentID
        })?.key { return existing }
        let authority = AttachmentAuthority(
            roomID: roomID,
            messageID: messageID,
            attachmentID: attachmentID
        )
        let reference = sealedReference(
            prefix: "attachment_",
            kind: "attachment",
            authority: authority
        )
        attachments[reference] = authority
        return reference
    }

    func attachmentAuthority(reference: String) -> AttachmentAuthority? {
        attachments[reference] ?? openedReference(
            reference,
            prefix: "attachment_",
            kind: "attachment",
            as: AttachmentAuthority.self
        )
    }

    func teamAssetReference(assetID: String, teamRoomID: String, revision: Int) -> String {
        if let existing = teamAssets.first(where: {
            $0.value.assetID == assetID
                && $0.value.teamRoomID == teamRoomID
                && $0.value.revision == revision
        })?.key { return existing }
        let authority = TeamAssetAuthority(
            assetID: assetID,
            teamRoomID: teamRoomID,
            revision: revision
        )
        let reference = sealedReference(
            prefix: "team_asset_",
            kind: "team_asset",
            authority: authority
        )
        teamAssets[reference] = authority
        return reference
    }

    func teamAssetAuthority(reference: String) -> TeamAssetAuthority? {
        teamAssets[reference] ?? openedReference(
            reference,
            prefix: "team_asset_",
            kind: "team_asset",
            as: TeamAssetAuthority.self
        )
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

    private func sealedReference<Value: Codable & Sendable>(
        prefix: String,
        kind: String,
        authority: Value
    ) -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let payload = SealedReference(version: 1, kind: kind, authority: authority)
        guard let plaintext = try? encoder.encode(payload),
              let combined = try? AES.GCM.seal(plaintext, using: referenceKey).combined else {
            preconditionFailure("Unable to create an opaque local Agent reference")
        }
        return prefix + Self.base64URL(combined)
    }

    private func openedReference<Value: Codable & Sendable>(
        _ reference: String,
        prefix: String,
        kind: String,
        as _: Value.Type
    ) -> Value? {
        guard reference.hasPrefix(prefix),
              let combined = Self.dataFromBase64URL(String(reference.dropFirst(prefix.count))),
              let sealedBox = try? AES.GCM.SealedBox(combined: combined),
              let plaintext = try? AES.GCM.open(sealedBox, using: referenceKey),
              let payload = try? JSONDecoder().decode(SealedReference<Value>.self, from: plaintext),
              payload.version == 1,
              payload.kind == kind else { return nil }
        return payload.authority
    }

    private static func base64URL(_ data: Data) -> String {
        data.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }

    private static func dataFromBase64URL(_ value: String) -> Data? {
        var base64 = value
            .replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        let remainder = base64.count % 4
        if remainder != 0 {
            base64.append(String(repeating: "=", count: 4 - remainder))
        }
        return Data(base64Encoded: base64)
    }
}
