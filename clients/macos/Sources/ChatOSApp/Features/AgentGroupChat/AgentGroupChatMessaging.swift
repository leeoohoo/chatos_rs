import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import Foundation

@MainActor
extension AgentGroupChatViewModel {
    func sendMessage() async {
        guard let room, !isSending else { return }
        let content = draftMessage.trimmingCharacters(in: .whitespacesAndNewlines)
        let outgoingAttachments = attachments
        guard !content.isEmpty || !outgoingAttachments.isEmpty else { return }
        guard let mentionedAgentIDs = resolvedMentionAgentIDs(in: content) else { return }
        isSending = true
        defer { isSending = false }
        do {
            let store = try await resolveStore()
            let post = try await store.postMessage(
                ownerUserID: ownerUserID,
                roomID: room.id,
                draft: .init(
                    senderKind: .human,
                    senderID: ownerUserID,
                    content: content,
                    mentionedAgentIDs: mentionedAgentIDs.sorted(),
                    attachments: outgoingAttachments.map(ProjectAgentMessageAttachmentDraft.init)
                ),
                limits: .init()
            )
            draftMessage = ""
            attachments = []
            attachmentError = nil
            selectedMentionAgentIDs.removeAll()
            await load()
            scrollToLatestRequest &+= 1
            if !post.deliveries.isEmpty {
                startScheduler()
            }
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func toggleMention(agentID: String) {
        if selectedMentionAgentIDs.contains(agentID) {
            selectedMentionAgentIDs.remove(agentID)
        } else {
            selectedMentionAgentIDs.insert(agentID)
        }
    }

    func selectMention(agentID: String) {
        guard activeMembers.contains(where: { $0.member.agentID == agentID }) else { return }
        selectedMentionAgentIDs.insert(agentID)
    }

    func resolvedMentionAgentIDs(in content: String) -> Set<String>? {
        var result = selectedMentionAgentIDs
        let namedMembers = activeMembers.compactMap { item -> (String, String)? in
            guard let name = item.profile?.draft.name.trimmingCharacters(in: .whitespacesAndNewlines),
                  !name.isEmpty else { return nil }
            return (name, item.member.agentID)
        }
        let grouped = Dictionary(grouping: namedMembers) {
            $0.0.folding(
                options: [.caseInsensitive, .diacriticInsensitive],
                locale: .current
            )
        }
        for members in grouped.values {
            guard let name = members.first?.0,
                  AgentChatMentionSyntax.containsMention(named: name, in: content) else {
                continue
            }
            if members.count == 1, let agentID = members.first?.1 {
                result.insert(agentID)
            } else if result.isDisjoint(with: Set(members.map(\.1))) {
                errorMessage = "团队中有多个 Agent 名为“\(name)”，请从 @ 候选列表选择具体成员。"
                return nil
            }
        }
        return result
    }

    func resumeRun(deliveryID: String) async {
        guard !isRunningAgents, runActionDeliveryIDs.insert(deliveryID).inserted else { return }
        defer { runActionDeliveryIDs.remove(deliveryID) }
        do {
            let result = try await scheduler.resumeDelivery(
                ownerUserID: ownerUserID,
                projectID: projectID,
                deliveryID: deliveryID
            )
            await load()
            switch result.outcome {
            case .completed:
                startScheduler()
            case .suspended, .failed:
                errorMessage = result.detail ?? "本地 Agent Run 尚未完成。"
            }
        } catch {
            await load()
            errorMessage = error.localizedDescription
        }
    }

    func abandonRun(deliveryID: String) async {
        guard !isRunningAgents, runActionDeliveryIDs.insert(deliveryID).inserted else { return }
        defer { runActionDeliveryIDs.remove(deliveryID) }
        do {
            try await scheduler.abandonDelivery(
                ownerUserID: ownerUserID,
                projectID: projectID,
                deliveryID: deliveryID
            )
            await load()
            startScheduler()
        } catch {
            await load()
            errorMessage = error.localizedDescription
        }
    }

    func pauseAgents() async {
        guard isRunningAgents, !isPausingAgents, !isStoppingAgents else { return }
        isPausingAgents = true
        schedulerNeedsAnotherPass = false
        communicationSchedulerNeedsAnotherPass = false
        let activeTask = schedulerTask
        let activeCommunicationTask = communicationSchedulerTask
        activeTask?.cancel()
        activeCommunicationTask?.cancel()
        await activeTask?.value
        await activeCommunicationTask?.value
        await load()
        isPausingAgents = false
    }

    func stopAllAgents() async {
        guard !isStoppingAgents else { return }
        isStoppingAgents = true
        schedulerNeedsAnotherPass = false
        communicationSchedulerNeedsAnotherPass = false
        let activeTask = schedulerTask
        let activeCommunicationTask = communicationSchedulerTask
        activeTask?.cancel()
        activeCommunicationTask?.cancel()
        await activeTask?.value
        await activeCommunicationTask?.value
        do {
            _ = try await scheduler.stopProject(
                ownerUserID: ownerUserID,
                projectID: projectID
            )
            await load()
            errorMessage = nil
        } catch {
            await load()
            errorMessage = error.localizedDescription
        }
        isStoppingAgents = false
    }

    func resolveStore() async throws -> SQLiteAgentGroupChatStore {
        if let openedStore { return openedStore }
        let store = try await service.store()
        openedStore = store
        return store
    }

    func loadAttachmentData(
        messages: [ProjectAgentMessage],
        roomID: String,
        store: SQLiteAgentGroupChatStore
    ) async throws -> [String: Data] {
        var result: [String: Data] = [:]
        for message in messages {
            for attachment in message.attachmentItems where attachment.kind == .image {
                guard let payload = try await store.messageAttachment(
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    messageID: message.id,
                    attachmentID: attachment.id
                ) else { continue }
                result[attachment.id] = try Data(
                    contentsOf: payload.localFileURL,
                    options: [.mappedIfSafe]
                )
            }
        }
        return result
    }

    func mergeMessages(
        _ current: [ProjectAgentMessage],
        with incoming: [ProjectAgentMessage]
    ) -> [ProjectAgentMessage] {
        var byID = Dictionary(uniqueKeysWithValues: current.map { ($0.id, $0) })
        for message in incoming {
            byID[message.id] = message
        }
        return byID.values.sorted {
            ($0.createdAtUnixMs, $0.id) < ($1.createdAtUnixMs, $1.id)
        }
    }

    func startScheduler() {
        if let roomID = room?.id {
            startCommunicationScheduler(roomID: roomID)
        }
        schedulerNeedsAnotherPass = true
        guard schedulerTask == nil else { return }
        isRunningAgents = true
        schedulerTask = Task { [weak self] in
            guard let self else { return }
            repeat {
                schedulerNeedsAnotherPass = false
                do {
                    let results = try await scheduler.drainAccount(ownerUserID: ownerUserID)
                    if let roomID = room?.id {
                        try await reconcileSchedulerResults(results, roomID: roomID)
                    }
                } catch is CancellationError {
                    // Account draining is process-wide. A competing surface owning the lease is
                    // normal and its durable updates will be observed by this room.
                } catch {
                    errorMessage = error.localizedDescription
                }
                await load()
                NotificationCenter.default.post(name: .agentGroupChatRoomsDidChange, object: nil)
            } while schedulerNeedsAnotherPass
            isRunningAgents = false
            schedulerTask = nil
        }
    }

    private func startCommunicationScheduler(roomID: String) {
        communicationSchedulerNeedsAnotherPass = true
        guard communicationSchedulerTask == nil else { return }
        communicationSchedulerTask = Task { [weak self] in
            guard let self else { return }
            repeat {
                communicationSchedulerNeedsAnotherPass = false
                do {
                    let results = try await scheduler.drainCommunication(
                        ownerUserID: ownerUserID,
                        roomID: roomID
                    )
                    try await reconcileSchedulerResults(results, roomID: roomID)
                } catch is CancellationError {
                    break
                } catch {
                    errorMessage = error.localizedDescription
                }
                await load()
                NotificationCenter.default.post(name: .agentGroupChatRoomsDidChange, object: nil)
            } while communicationSchedulerNeedsAnotherPass && !Task.isCancelled
            communicationSchedulerTask = nil
        }
    }
}
