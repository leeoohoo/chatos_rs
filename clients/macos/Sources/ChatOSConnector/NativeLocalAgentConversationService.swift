import ChatOSCore
import Foundation

public actor NativeLocalAgentConversationService:
    ConversationCommandServicing,
    ConversationRemoteServicing,
    ConversationRealtimeStreaming
{
    private struct Context: Sendable {
        let ownerUserID: String
        let capabilitiesByModelConfigRef: [String: LocalAgentCapabilityPolicySnapshot]
        let pluginInstallerCapabilitiesByModelConfigRef: [
            String: LocalAgentCapabilityPolicySnapshot
        ]
    }

    private let client: NativeLocalAgentConversationClient
    private let runtimeClient: NativeLocalAgentRuntimeClient
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
        self.runtimeClient = NativeLocalAgentRuntimeClient(host: host)
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
              bootstrap.capabilitySnapshot.ownerUserID == ownerUserID,
              bootstrap.modelSnapshots.allSatisfy({ snapshot in
                  bootstrap.capabilitySnapshot(forModelConfigRef: snapshot.modelConfigRef)?
                      .ownerUserID == ownerUserID
                      && bootstrap.pluginInstallerCapabilitySnapshot(
                          forModelConfigRef: snapshot.modelConfigRef
                      )?.ownerUserID == ownerUserID
              }) else {
            throw NativeLocalAgentConversationServiceError.notConfigured
        }
        context = .init(
            ownerUserID: ownerUserID,
            capabilitiesByModelConfigRef: bootstrap.capabilitySnapshotsByModelConfigRef,
            pluginInstallerCapabilitiesByModelConfigRef:
                bootstrap.pluginInstallerCapabilitySnapshotsByModelConfigRef
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
        let isPluginInstallerConversation = Self.usesPluginInstallerCapability(
            for: conversation.conversation.resource
        )
        let capabilityMap = isPluginInstallerConversation
            ? context.pluginInstallerCapabilitiesByModelConfigRef
            : context.capabilitiesByModelConfigRef
        guard let capability = capabilityMap[runtimeSelection.modelSnapshot.modelConfigRef] else {
            throw NativeLocalAgentConversationServiceError.notConfigured
        }
        let messageID = "message_\(UUID().uuidString.lowercased())"
        let attachments = try attachmentVault.authorize(
            command.attachments,
            ownerUserID: context.ownerUserID,
            conversationID: command.sessionID
        )
        var messageMetadata: [String: LocalAgentJSONValue] = [
            "source": .string(
                isPluginInstallerConversation
                    ? "native_plugin_installer_chat"
                    : "native_main_chat"
            ),
            "reasoning_enabled": .bool(command.reasoningEnabled ?? false),
        ]
        if let selection = command.explicitCapabilitySelection {
            var explicitSelection: [String: LocalAgentJSONValue] = [
                "kind": .string(selection.kind.rawValue),
                "display_name": .string(selection.displayName),
            ]
            if let pluginKey = selection.pluginKey {
                explicitSelection["plugin_key"] = .string(pluginKey)
            }
            if let componentKey = selection.componentKey {
                explicitSelection["component_key"] = .string(componentKey)
            }
            messageMetadata["explicit_capability_selection"] = .object(explicitSelection)
        }
        let result = try await client.startTurn(.init(
            ownerUserID: context.ownerUserID,
            conversationID: command.sessionID,
            expectedConversationVersion: conversation.conversation.version,
            turnID: command.turnID,
            messageID: messageID,
            runID: "run_\(UUID().uuidString.lowercased())",
            message: command.content,
            messageMetadata: .object(messageMetadata),
            attachments: attachments,
            modelConfigRef: runtimeSelection.modelSnapshot.modelConfigRef,
            modelConfigRevision: runtimeSelection.modelSnapshot.modelConfigRevision,
            capabilityPolicyRevision: capability.capabilityPolicyRevision
        ))
        await platformToolWorker?.wake()
        return .init(
            accepted: true,
            turnID: result.turn.turnID,
            userMessageID: result.message?.messageID ?? messageID
        )
    }

    static func usesPluginInstallerCapability(
        for resource: LocalAgentConversationResourceBinding?
    ) -> Bool {
        resource == .init(
            kind: .contact,
            resourceID: WorkspaceSystemContact.pluginInstallerID
        )
    }

    public func sendGuidance(
        _ command: ConversationSendCommand
    ) async throws -> ConversationCommandAck {
        let context = try requireContext()
        let detail = try await client.context(
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
        let detail = try await client.context(
            ownerUserID: context.ownerUserID,
            conversationID: conversationID
        )
        let target: LocalAgentConversationTurnRecord?
        if let turnID {
            // A stale pet card must never cancel a different, newer Turn.
            target = detail.turns.first(where: { $0.turnID == turnID })
        } else {
            target = detail.turns.last(where: { $0.status == "running" })
        }
        guard let target else { return }
        var expectedRunVersion: UInt64?
        if target.status != "running" {
            // NeedsReview reconciles the visible Turn to failed, while its Run
            // remains cancellable. Match the Host's reviewed-Turn action gate.
            guard target.status == "failed" else { return }
            let run = try await runtimeClient.run(
                ownerUserID: context.ownerUserID,
                runID: target.runID
            )
            guard run.status == "needs_review" else { return }
            guard run.ownerEntityType == "conversation_turn",
                  run.ownerEntityID == target.turnID else {
                throw NativeLocalAgentHostError.invalidResponse
            }
            expectedRunVersion = run.version
        }
        _ = try await client.cancelTurn(
            ownerUserID: context.ownerUserID,
            conversationID: conversationID,
            expectedConversationVersion: detail.conversation.version,
            turnID: target.turnID,
            expectedRunVersion: expectedRunVersion,
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
                    let detail = try await client.context(
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
                            for signal in Self.askUserPromptSignals(
                                events,
                                conversationID: sessionID
                            ) {
                                continuation.yield(signal)
                            }
                        }

                        let detail = try await client.context(
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
            return try await client.context(
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

    static func askUserPromptSignals(
        _ events: [LocalAgentEventRecord],
        conversationID: String
    ) -> [ConversationRealtimeSignal] {
        events.compactMap { event in
            guard event.eventType == "user_input_requested",
                  case let .object(payload)? = event.payload,
                  case let .string(eventConversationID)? = payload["conversation_id"],
                  eventConversationID == conversationID else { return nil }
            let turnID: String? = if case let .string(value)? = payload["turn_id"] {
                value
            } else {
                nil
            }
            return ConversationRealtimeSignal(
                eventID: event.eventID,
                eventSequence: event.cursor,
                sessionID: conversationID,
                turnID: turnID,
                kind: .updated,
                eventName: event.eventType,
                timestamp: timestamp(unixMilliseconds: event.createdAtUnixMs),
                askUserPromptUpdate: AskUserPromptRealtimeUpdate(
                    promptID: "local-ask:\(event.runID)",
                    sessionID: conversationID,
                    turnID: turnID,
                    action: "created",
                    status: .pending
                )
            )
        }
    }

    private static func timestamp(unixMilliseconds: Int64) -> String {
        ISO8601DateFormatter().string(
            from: Date(timeIntervalSince1970: Double(unixMilliseconds) / 1_000)
        )
    }

    func mapTurns(_ page: LocalAgentConversationHistoryPage) -> [ConversationTurn] {
        let messagesByTurn = Dictionary(grouping: page.messages, by: \.turnID)
        let attachmentsByMessage = Dictionary(grouping: page.attachments, by: \.messageID)
        return page.turns.map { turn in
            let messages = (messagesByTurn[turn.turnID] ?? []).sorted { $0.ordinal < $1.ordinal }
            let user = messages.first(where: { $0.role == "user" })
            let additionalUsers = messages.filter { $0.role == "user" }.dropFirst().map { message in
                mapMessage(
                    message,
                    fallbackID: message.messageID,
                    attachments: attachmentsByMessage[message.messageID] ?? [],
                    ownerUserID: page.conversation.ownerUserID,
                    conversationID: turn.conversationID
                )
            }
            let assistants = Self.coalescingTaskGraphTerminalMessages(
                messages.filter { $0.role == "assistant" }
            )
            let userMessage = mapMessage(
                user,
                fallbackID: turn.userMessageID,
                attachments: attachmentsByMessage[turn.userMessageID] ?? [],
                ownerUserID: page.conversation.ownerUserID,
                conversationID: turn.conversationID
            )
            let projectedReplies = assistants.flatMap { message in
                message.replyProjections(sourceUserMessageID: turn.userMessageID).map { projection in
                    ConversationAssistantReply(
                        message: mapMessage(
                            message,
                            fallbackID: projection.messageID,
                            attachments: attachmentsByMessage[message.messageID] ?? [],
                            ownerUserID: page.conversation.ownerUserID,
                            conversationID: turn.conversationID,
                            projectedText: projection.text
                        ),
                        taskCallback: projection.taskCallback
                    )
                }
            }
            // Preserve the original Task Runner presentation contract: the Main Chat
            // handoff reply is rendered first, then each Task/Run callback. Storage
            // ordinals can interleave because local scheduling starts immediately.
            let replies = Self.orderedConversationReplies(projectedReplies)
            let finalReplies = replies.filter { $0.taskCallback == nil }
            let status = mapStatus(turn.status)
            return ConversationTurn(
                id: turn.turnID,
                sessionID: turn.conversationID,
                sequence: Int64(clamping: user?.ordinal ?? 0),
                revision: Int64(clamping: page.conversation.version),
                userMessage: userMessage,
                additionalUserMessages: additionalUsers,
                processEvents: [TurnProcessEvent(
                    id: "local-process:\(turn.runID):\(turn.updatedAtUnixMs)",
                    title: processTitle(status),
                    status: status
                )],
                finalAssistantMessage: finalReplies.last?.message,
                assistantReplies: replies,
                messageTaskLookup: MessageTaskLookup(
                    sessionID: turn.conversationID,
                    turnID: turn.turnID,
                    sourceUserMessageID: userMessage.id
                ),
                isTaskGraphAvailable: true,
                status: status,
                startedAt: Date(timeIntervalSince1970: Double(turn.createdAtUnixMs) / 1_000),
                completedAt: status == .streaming ? nil
                    : Date(timeIntervalSince1970: Double(turn.updatedAtUnixMs) / 1_000)
            )
        }
    }

    /// Storage retains every terminal generation for audit and retry history. Main Chat only
    /// presents the newest generation for each Task Graph so a later cancellation or recovery
    /// does not repeat the previous terminal summary and its full delivery.
    static func coalescingTaskGraphTerminalMessages(
        _ messages: [LocalAgentConversationMessageRecord]
    ) -> [LocalAgentConversationMessageRecord] {
        var latestOrdinalByGraphID: [String: UInt64] = [:]
        for message in messages {
            guard let graphID = message.taskGraphTerminalGraphID else { continue }
            latestOrdinalByGraphID[graphID] = max(
                latestOrdinalByGraphID[graphID] ?? 0,
                message.ordinal
            )
        }
        return messages.filter { message in
            guard let graphID = message.taskGraphTerminalGraphID else { return true }
            return latestOrdinalByGraphID[graphID] == message.ordinal
        }
    }

    static func orderedConversationReplies(
        _ replies: [ConversationAssistantReply]
    ) -> [ConversationAssistantReply] {
        // Keep the handoff acknowledgement first, but preserve durable ordinal
        // order among progress messages and graph deliveries. Graph delivery IDs
        // are minted by the Host (task-graph:<graph>:<generation>), not user text.
        let isAsyncTaskReply: (ConversationAssistantReply) -> Bool = {
            $0.taskCallback != nil || $0.id.hasPrefix("task-graph:")
        }
        let mainChat = replies.filter { !isAsyncTaskReply($0) }
        let taskReplies = replies.filter(isAsyncTaskReply)
        return mainChat + taskReplies
    }

    private func mapMessage(
        _ message: LocalAgentConversationMessageRecord?,
        fallbackID: String,
        attachments: [LocalAgentConversationAttachmentRecord],
        ownerUserID: String,
        conversationID: String,
        projectedText: String? = nil
    ) -> ChatMessage {
        ChatMessage(
            id: projectedText == nil ? (message?.messageID ?? fallbackID) : fallbackID,
            role: message?.role == "assistant" ? .assistant : .user,
            text: projectedText ?? message?.content.conversationDisplayText ?? "",
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
            },
            storageOrdinal: message?.ordinal
        )
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
