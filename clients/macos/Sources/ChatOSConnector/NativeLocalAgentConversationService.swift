import ChatOSCore
import CryptoKit
import Foundation

public actor NativeLocalAgentConversationService:
    ConversationCommandServicing,
    ConversationRemoteServicing,
    ConversationRealtimeStreaming
{
    private struct Context: Sendable {
        let ownerUserID: String
        let model: LocalAgentModelConfigSnapshot
        let capability: LocalAgentCapabilityPolicySnapshot
    }

    private let client: NativeLocalAgentConversationClient
    private let attachmentVault: NativeLocalAgentAttachmentVault
    private var context: Context?

    public init(host: any LocalAgentHostClientServicing, attachmentRootURL: URL) {
        self.client = NativeLocalAgentConversationClient(host: host)
        self.attachmentVault = NativeLocalAgentAttachmentVault(rootURL: attachmentRootURL)
    }

    public func configure(
        ownerUserID: String,
        bootstrap: NativeLocalAgentBootstrapResult
    ) throws {
        guard let model = bootstrap.modelSnapshots.first,
              bootstrap.capabilitySnapshot.ownerUserID == ownerUserID else {
            throw NativeLocalAgentConversationServiceError.notConfigured
        }
        context = .init(
            ownerUserID: ownerUserID,
            model: model,
            capability: bootstrap.capabilitySnapshot
        )
    }

    public func reset() {
        context = nil
    }

    public func sendNewTurn(
        _ command: ConversationSendCommand
    ) async throws -> ConversationCommandAck {
        let context = try requireContext()
        let conversation = try await ensureConversation(
            ownerUserID: context.ownerUserID,
            conversationID: command.sessionID
        )
        let messageID = "message_\(UUID().uuidString.lowercased())"
        let attachments = try attachmentVault.authorize(
            command.attachments,
            ownerUserID: context.ownerUserID,
            conversationID: command.sessionID
        )
        let result = try await client.startTurn(.init(
            ownerUserID: context.ownerUserID,
            conversationID: command.sessionID,
            expectedConversationVersion: conversation.conversation.version,
            turnID: command.turnID,
            messageID: messageID,
            runID: "run_\(UUID().uuidString.lowercased())",
            message: command.content,
            messageMetadata: .object([
                "source": .string("native_main_chat"),
                "reasoning_enabled": .bool(command.reasoningEnabled ?? false),
            ]),
            attachments: attachments,
            modelConfigRef: context.model.modelConfigRef,
            modelConfigRevision: context.model.modelConfigRevision,
            capabilityPolicyRevision: context.capability.capabilityPolicyRevision
        ))
        return .init(
            accepted: true,
            turnID: result.turn.turnID,
            userMessageID: result.message?.messageID ?? messageID
        )
    }

    public func sendGuidance(
        _ command: ConversationSendCommand
    ) async throws -> ConversationCommandAck {
        let context = try requireContext()
        let detail = try await client.get(
            ownerUserID: context.ownerUserID,
            conversationID: command.sessionID
        )
        guard detail.turns.contains(where: {
            $0.turnID == command.turnID && $0.status == "running"
        }) else {
            throw ConversationCommandError.guidanceTargetInactive
        }
        let messageID = "message_\(UUID().uuidString.lowercased())"
        let attachments = try attachmentVault.authorize(
            command.attachments,
            ownerUserID: context.ownerUserID,
            conversationID: command.sessionID
        )
        let result = try await client.guideTurn(.init(
            ownerUserID: context.ownerUserID,
            conversationID: command.sessionID,
            expectedConversationVersion: detail.conversation.version,
            turnID: command.turnID,
            expectedRunVersion: nil,
            messageID: messageID,
            message: command.content,
            messageMetadata: .object(["source": .string("native_guidance")]),
            attachments: attachments
        ))
        return .init(
            accepted: true,
            turnID: result.turn.turnID,
            userMessageID: result.message?.messageID ?? messageID
        )
    }

    public func stopTurn(conversationID: String, turnID: String?) async throws {
        let context = try requireContext()
        let detail = try await client.get(
            ownerUserID: context.ownerUserID,
            conversationID: conversationID
        )
        let target = turnID.flatMap { id in detail.turns.first(where: { $0.turnID == id }) }
            ?? detail.turns.last(where: { $0.status == "running" })
        guard let target, target.status == "running" else {
            throw ConversationCommandError.guidanceTargetInactive
        }
        _ = try await client.cancelTurn(
            ownerUserID: context.ownerUserID,
            conversationID: conversationID,
            expectedConversationVersion: detail.conversation.version,
            turnID: target.turnID,
            expectedRunVersion: nil,
            reason: "user requested stop"
        )
    }

    public func fetchHistory(_ query: ConversationHistoryQuery) async throws -> HistoryPage {
        let context = try requireContext()
        let before = try query.before.map { cursor in
            guard let value = UInt64(cursor), value > 0 else {
                throw NativeLocalAgentConversationServiceError.invalidCursor
            }
            return value
        }
        let page: LocalAgentConversationHistoryPage
        do {
            page = try await client.history(
                ownerUserID: context.ownerUserID,
                conversationID: query.sessionID,
                beforeOrdinal: before,
                limit: UInt32(max(1, min(100, query.limit)))
            )
        } catch let error as NativeLocalAgentHostError {
            if case let .hostError(code, _, _) = error, code == "not_found" {
                _ = try await client.create(
                    ownerUserID: context.ownerUserID,
                    conversationID: query.sessionID,
                    title: "Conversation"
                )
                page = try await client.history(
                    ownerUserID: context.ownerUserID,
                    conversationID: query.sessionID,
                    beforeOrdinal: before,
                    limit: UInt32(max(1, min(100, query.limit)))
                )
            } else {
                throw error
            }
        }
        return HistoryPage(
            turns: mapTurns(page),
            olderCursor: page.nextBeforeOrdinal.map(String.init),
            hasOlder: page.nextBeforeOrdinal != nil,
            snapshotRevision: Int64(clamping: page.conversation.version),
            requestGeneration: query.requestGeneration
        )
    }

    public func issueWebSocketTicket() async throws -> String {
        throw NativeLocalAgentConversationServiceError.realtimeUnavailable
    }

    public func events(
        sessionID: String
    ) async -> AsyncThrowingStream<ConversationRealtimeSignal, Error> {
        guard let context else {
            return AsyncThrowingStream { continuation in
                continuation.finish(
                    throwing: NativeLocalAgentConversationServiceError.notConfigured
                )
            }
        }
        let client = client
        return AsyncThrowingStream { continuation in
            let pollingTask = Task {
                var observedVersion: UInt64?
                while !Task.isCancelled {
                    do {
                        let detail = try await client.get(
                            ownerUserID: context.ownerUserID,
                            conversationID: sessionID
                        )
                        let version = detail.conversation.version
                        if observedVersion != version {
                            observedVersion = version
                            continuation.yield(.init(
                                eventID: "local-\(sessionID)-\(version)",
                                eventSequence: Int64(clamping: version),
                                sessionID: sessionID,
                                turnID: nil,
                                kind: .reconcile,
                                eventName: "local_conversation_changed",
                                timestamp: Self.timestamp(
                                    unixMilliseconds: detail.conversation.updatedAtUnixMs
                                )
                            ))
                        }
                    } catch let error as NativeLocalAgentHostError {
                        guard Self.isNotFound(error) else {
                            continuation.finish(throwing: error)
                            return
                        }
                    } catch {
                        continuation.finish(throwing: error)
                        return
                    }

                    do {
                        try await Task.sleep(for: .milliseconds(400))
                    } catch {
                        return
                    }
                }
            }
            continuation.onTermination = { _ in pollingTask.cancel() }
        }
    }

    private func ensureConversation(
        ownerUserID: String,
        conversationID: String
    ) async throws -> LocalAgentConversationDetail {
        do {
            return try await client.get(
                ownerUserID: ownerUserID,
                conversationID: conversationID
            )
        } catch let error as NativeLocalAgentHostError {
            guard case let .hostError(code, _, _) = error, code == "not_found" else {
                throw error
            }
            return try await client.create(
                ownerUserID: ownerUserID,
                conversationID: conversationID,
                title: "Conversation"
            )
        }
    }

    private func requireContext() throws -> Context {
        guard let context else {
            throw NativeLocalAgentConversationServiceError.notConfigured
        }
        return context
    }

    private static func isNotFound(_ error: NativeLocalAgentHostError) -> Bool {
        if case let .hostError(code, _, _) = error {
            return code == "not_found"
        }
        return false
    }

    private static func timestamp(unixMilliseconds: Int64) -> String {
        ISO8601DateFormatter().string(
            from: Date(timeIntervalSince1970: Double(unixMilliseconds) / 1_000)
        )
    }

    private func mapTurns(_ page: LocalAgentConversationHistoryPage) -> [ConversationTurn] {
        let messagesByTurn = Dictionary(grouping: page.messages, by: \.turnID)
        let attachmentsByMessage = Dictionary(grouping: page.attachments, by: \.messageID)
        return page.turns.map { turn in
            let messages = (messagesByTurn[turn.turnID] ?? []).sorted { $0.ordinal < $1.ordinal }
            let user = messages.first(where: { $0.role == "user" })
            let assistants = messages.filter { $0.role == "assistant" }
            let userMessage = mapMessage(
                user,
                fallbackID: turn.userMessageID,
                attachments: attachmentsByMessage[turn.userMessageID] ?? []
            )
            let replies = assistants.map { message in
                ConversationAssistantReply(message: mapMessage(
                    message,
                    fallbackID: message.messageID,
                    attachments: attachmentsByMessage[message.messageID] ?? []
                ))
            }
            let status = mapStatus(turn.status)
            return ConversationTurn(
                id: turn.turnID,
                sessionID: turn.conversationID,
                sequence: Int64(clamping: user?.ordinal ?? 0),
                revision: Int64(clamping: page.conversation.version),
                userMessage: userMessage,
                finalAssistantMessage: replies.last?.message,
                assistantReplies: replies,
                isTaskGraphAvailable: true,
                status: status,
                startedAt: Date(timeIntervalSince1970: Double(turn.createdAtUnixMs) / 1_000),
                completedAt: status == .streaming ? nil
                    : Date(timeIntervalSince1970: Double(turn.updatedAtUnixMs) / 1_000)
            )
        }
    }

    private func mapMessage(
        _ message: LocalAgentConversationMessageRecord?,
        fallbackID: String,
        attachments: [LocalAgentConversationAttachmentRecord]
    ) -> ChatMessage {
        ChatMessage(
            id: message?.messageID ?? fallbackID,
            role: message?.role == "assistant" ? .assistant : .user,
            text: message.map { text(from: $0.content) } ?? "",
            createdAt: Date(
                timeIntervalSince1970: Double(message?.createdAtUnixMs ?? 0) / 1_000
            ),
            attachments: attachments.map {
                ConversationAttachmentReference(
                    id: $0.attachmentID,
                    name: $0.displayName,
                    mimeType: $0.mediaType,
                    size: Int(clamping: $0.byteSize),
                    kind: $0.mediaType.hasPrefix("image/") ? .image : .file
                )
            }
        )
    }

    private func text(from value: LocalAgentJSONValue) -> String {
        switch value {
        case .null: ""
        case let .string(value): value
        case let .bool(value): String(value)
        case let .number(value): String(value)
        case let .array(values): values.map(text).joined(separator: "\n")
        case let .object(values):
            values["text"].map(text)
                ?? values["content"].map(text)
                ?? ""
        }
    }

    private func mapStatus(_ value: String) -> TurnStatus {
        switch value {
        case "running": .streaming
        case "succeeded": .completed
        case "cancelled": .cancelled
        default: .failed
        }
    }
}

private struct NativeLocalAgentAttachmentVault: Sendable {
    let rootURL: URL

    func authorize(
        _ drafts: [ConversationAttachmentDraft],
        ownerUserID: String,
        conversationID: String
    ) throws -> [LocalAgentConversationAttachmentSpec] {
        try drafts.map { draft in
            let token = UUID().uuidString.lowercased()
            let directory = rootURL
                .appendingPathComponent(safe(ownerUserID), isDirectory: true)
                .appendingPathComponent(safe(conversationID), isDirectory: true)
            try FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: 0o700]
            )
            let file = directory.appendingPathComponent(token, isDirectory: false)
            try draft.data.write(to: file, options: .atomic)
            try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: file.path)
            let digest = SHA256.hash(data: draft.data)
                .map { String(format: "%02x", $0) }
                .joined()
            return .init(
                attachmentID: draft.id,
                displayName: draft.name,
                mediaType: draft.mimeType,
                byteSize: UInt64(draft.data.count),
                sha256: digest,
                authorizedLocalRef: "local-attachment:\(token)",
                metadata: .object(["kind": .string(draft.kind.rawValue)])
            )
        }
    }

    private func safe(_ value: String) -> String {
        value.unicodeScalars.map { scalar in
            CharacterSet.alphanumerics.contains(scalar) || scalar == "-" ? String(scalar) : "_"
        }.joined()
    }
}

public enum NativeLocalAgentConversationServiceError: LocalizedError {
    case notConfigured
    case invalidCursor
    case realtimeUnavailable

    public var errorDescription: String? {
        switch self {
        case .notConfigured: "Local Agent conversation service is not configured."
        case .invalidCursor: "Local Agent conversation history cursor is invalid."
        case .realtimeUnavailable: "Local Agent realtime uses local refresh, not WebSocket tickets."
        }
    }
}
