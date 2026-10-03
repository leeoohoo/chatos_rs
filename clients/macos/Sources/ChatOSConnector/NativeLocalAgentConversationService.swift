import ChatOSCore
import Foundation

public actor NativeLocalAgentConversationService:
    ConversationCommandServicing,
    ConversationRemoteServicing,
    ConversationRealtimeStreaming
{
    private struct Context: Sendable {
        let ownerUserID: String
        let capability: LocalAgentCapabilityPolicySnapshot
    }

    private let client: NativeLocalAgentConversationClient
    private let eventHub: NativeLocalAgentEventHub
    private let attachmentVault: NativeLocalAgentAttachmentVault
    private let runtimeSettings: NativeLocalAgentConversationRuntimeSettingsService
    private let platformToolWorker: NativeLocalAgentPlatformToolWorker?
    private var context: Context?

    public init(
        host: any LocalAgentHostClientServicing,
        attachmentRootURL: URL,
        runtimeSettings: NativeLocalAgentConversationRuntimeSettingsService,
        platformToolWorker: NativeLocalAgentPlatformToolWorker? = nil,
        eventHub: NativeLocalAgentEventHub? = nil
    ) {
        self.client = NativeLocalAgentConversationClient(host: host)
        self.eventHub = eventHub ?? NativeLocalAgentEventHub(host: host)
        self.attachmentVault = NativeLocalAgentAttachmentVault(rootURL: attachmentRootURL)
        self.runtimeSettings = runtimeSettings
        self.platformToolWorker = platformToolWorker
    }

    public func configure(
        ownerUserID: String,
        bootstrap: NativeLocalAgentBootstrapResult
    ) async throws {
        guard !bootstrap.modelSnapshots.isEmpty,
              bootstrap.capabilitySnapshot.ownerUserID == ownerUserID else {
            throw NativeLocalAgentConversationServiceError.notConfigured
        }
        context = .init(
            ownerUserID: ownerUserID,
            capability: bootstrap.capabilitySnapshot
        )
        await eventHub.configure(ownerUserID: ownerUserID)
    }

    public func reset() async {
        context = nil
        await eventHub.reset()
    }

    public func sendNewTurn(
        _ command: ConversationSendCommand
    ) async throws -> ConversationCommandAck {
        let context = try requireContext()
        let runtimeSelection = try await runtimeSettings.resolveSelection(
            sessionID: command.sessionID
        )
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
            modelConfigRef: runtimeSelection.modelSnapshot.modelConfigRef,
            modelConfigRevision: runtimeSelection.modelSnapshot.modelConfigRevision,
            capabilityPolicyRevision: context.capability.capabilityPolicyRevision
        ))
        await platformToolWorker?.wake()
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
        await platformToolWorker?.wake()
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
        let updates = await eventHub.updates()
        return AsyncThrowingStream(bufferingPolicy: .bufferingNewest(1)) { continuation in
            let pollingTask = Task {
                var observedVersion: UInt64?
                var observedRunIDs = Set<String>()
                do {
                    let detail = try await client.get(
                        ownerUserID: context.ownerUserID,
                        conversationID: sessionID
                    )
                    let version = detail.conversation.version
                    observedVersion = version
                    observedRunIDs = Set(detail.turns.map(\.runID))
                    continuation.yield(Self.reconcileSignal(
                        detail: detail,
                        sessionID: sessionID
                    ))
                } catch let error as NativeLocalAgentHostError {
                    guard Self.isNotFound(error) else {
                        continuation.finish(throwing: error)
                        return
                    }
                } catch is CancellationError {
                    return
                } catch {
                    continuation.finish(throwing: error)
                    return
                }

                for await update in updates {
                    guard !Task.isCancelled,
                          update.ownerUserID == context.ownerUserID else { continue }
                    do {
                        if case let .events(events) = update.kind {
                            guard Self.eventsAffectConversation(
                                events,
                                conversationID: sessionID,
                                knownRunIDs: observedRunIDs
                            ) else { continue }
                        }

                        let detail = try await client.get(
                            ownerUserID: context.ownerUserID,
                            conversationID: sessionID
                        )
                        let version = detail.conversation.version
                        observedRunIDs = Set(detail.turns.map(\.runID))
                        guard observedVersion != version else { continue }
                        observedVersion = version
                        continuation.yield(Self.reconcileSignal(
                            detail: detail,
                            sessionID: sessionID
                        ))
                    } catch let error as NativeLocalAgentHostError {
                        guard Self.isNotFound(error) else {
                            continuation.finish(throwing: error)
                            return
                        }
                    } catch is CancellationError {
                        return
                    } catch {
                        continuation.finish(throwing: error)
                        return
                    }
                }
            }
            continuation.onTermination = { _ in pollingTask.cancel() }
        }
    }

    private static func reconcileSignal(
        detail: LocalAgentConversationDetail,
        sessionID: String
    ) -> ConversationRealtimeSignal {
        let version = detail.conversation.version
        return .init(
            eventID: "local-\(sessionID)-\(version)",
            eventSequence: Int64(clamping: version),
            sessionID: sessionID,
            turnID: nil,
            kind: .reconcile,
            eventName: "local_conversation_changed",
            timestamp: timestamp(
                unixMilliseconds: detail.conversation.updatedAtUnixMs
            )
        )
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

    static func eventsAffectConversation(
        _ events: [LocalAgentEventRecord],
        conversationID: String,
        knownRunIDs: Set<String>
    ) -> Bool {
        events.contains { event in
            if knownRunIDs.contains(event.runID) {
                return true
            }
            guard case let .object(payload)? = event.payload,
                  case let .string(eventConversationID)? = payload["conversation_id"] else {
                return false
            }
            return eventConversationID == conversationID
        }
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
                attachments: attachmentsByMessage[turn.userMessageID] ?? [],
                ownerUserID: page.conversation.ownerUserID,
                conversationID: turn.conversationID
            )
            let replies = assistants.map { message in
                ConversationAssistantReply(message: mapMessage(
                    message,
                    fallbackID: message.messageID,
                    attachments: attachmentsByMessage[message.messageID] ?? [],
                    ownerUserID: page.conversation.ownerUserID,
                    conversationID: turn.conversationID
                ))
            }
            let status = mapStatus(turn.status)
            return ConversationTurn(
                id: turn.turnID,
                sessionID: turn.conversationID,
                sequence: Int64(clamping: user?.ordinal ?? 0),
                revision: Int64(clamping: page.conversation.version),
                userMessage: userMessage,
                processEvents: [TurnProcessEvent(
                    id: "local-process:\(turn.runID):\(turn.updatedAtUnixMs)",
                    title: processTitle(status),
                    status: status
                )],
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
        attachments: [LocalAgentConversationAttachmentRecord],
        ownerUserID: String,
        conversationID: String
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
                    kind: $0.mediaType.hasPrefix("image/") ? .image : .file,
                    sha256: $0.sha256,
                    localURL: try? attachmentVault.previewURL(
                        $0,
                        ownerUserID: ownerUserID,
                        conversationID: conversationID
                    )
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

    private func processTitle(_ status: TurnStatus) -> String {
        switch status {
        case .queued: "本地执行等待中"
        case .streaming: "本地执行进行中"
        case .completed: "本地执行已完成"
        case .failed: "本地执行失败"
        case .cancelled: "本地执行已取消"
        }
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
