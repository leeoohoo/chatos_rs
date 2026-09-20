import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import SQLite3

extension SQLiteAgentGroupChatStore {
    public func postMessage(
        ownerUserID: String,
        roomID: String,
        draft: ProjectAgentMessageDraft,
        limits: AgentGroupChatRoutingLimits = .init()
    ) throws -> AgentGroupChatPostResult {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try draft.validate()
        try limits.validate()
        var attachmentDirectoryToRemove: URL?
        do {
            return try transaction {
            guard let room = try readRoom(ownerUserID: ownerUserID, roomID: roomID),
                  room.status == .active else {
                throw AgentGroupChatError.notFound
            }
            try validateSender(ownerUserID: ownerUserID, roomID: roomID, draft: draft)
            if let replyToMessageID = draft.replyToMessageID {
                try requireMessage(ownerUserID: ownerUserID, roomID: roomID, messageID: replyToMessageID)
            }
            if let rootMessageID = draft.rootMessageID {
                try requireMessage(ownerUserID: ownerUserID, roomID: roomID, messageID: rootMessageID)
            }
            for agentID in draft.mentionedAgentIDs {
                guard try readMember(ownerUserID: ownerUserID, roomID: roomID, agentID: agentID)?.status == .active else {
                    throw AgentGroupChatError.notMember
                }
            }

            let messageID = UUID().uuidString.lowercased()
            let rootMessageID = draft.rootMessageID ?? messageID
            let now = Self.now()
            try execute(
                """
                INSERT INTO project_agent_messages (
                    owner_user_id, id, room_id, sender_kind, sender_id, content,
                    reply_to_message_id, source_run_id, causation_id, root_message_id,
                    hop_count, created_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                [
                    .text(ownerUserID), .text(messageID), .text(roomID),
                    .text(draft.senderKind.rawValue), .text(draft.senderID), .text(draft.content),
                    .optionalText(draft.replyToMessageID), .optionalText(draft.sourceRunID),
                    .optionalText(draft.causationID), .text(rootMessageID),
                    .integer(Int64(draft.hopCount)), .integer(now),
                ]
            )
            for (position, agentID) in draft.mentionedAgentIDs.enumerated() {
                try execute(
                    """
                    INSERT INTO project_agent_message_mentions (
                        owner_user_id, message_id, agent_id, position
                    ) VALUES (?, ?, ?, ?)
                    """,
                    [
                        .text(ownerUserID), .text(messageID), .text(agentID),
                        .integer(Int64(position)),
                    ]
                )
            }

            if !draft.attachmentItems.isEmpty {
                attachmentDirectoryToRemove = attachmentDirectoryURL(messageID: messageID)
            }
            let persistedAttachments = try persistMessageAttachments(
                ownerUserID: ownerUserID,
                messageID: messageID,
                drafts: draft.attachmentItems,
                queueAgentArtifacts: draft.senderKind == .agent
            )

            let message = ProjectAgentMessage(
                id: messageID,
                ownerUserID: ownerUserID,
                roomID: roomID,
                draft: draft,
                rootMessageID: rootMessageID,
                attachments: persistedAttachments,
                createdAtUnixMs: now
            )
            let candidates: [String]
            if room.conversationKind.isDirect {
                candidates = try AgentConversationRepository.activeMemberIDs(
                    database,
                    ownerUserID: ownerUserID,
                    roomID: roomID,
                    preparedStatement: recordPreparedStatement
                )
            } else if !draft.mentionedAgentIDs.isEmpty {
                candidates = draft.mentionedAgentIDs
            } else if draft.senderKind == .human, let defaultAgentID = room.defaultAgentID {
                candidates = [defaultAgentID]
            } else {
                candidates = []
            }
            let targets = candidates.filter {
                !(draft.senderKind == .agent && $0 == draft.senderID)
            }
            let existingRunCount = try AgentDeliveryRepository.countForRootMessage(
                database,
                ownerUserID: ownerUserID,
                rootMessageID: rootMessageID,
                preparedStatement: recordPreparedStatement
            )
            let stopReason: String?
            if draft.hopCount > limits.maximumHopCount {
                stopReason = "本轮 Agent 协作已达到最大唤醒深度。"
            } else if existingRunCount + Int64(targets.count) > Int64(limits.maximumAgentRunsPerRootMessage) {
                stopReason = "本轮 Agent 协作已达到最大运行次数。"
            } else {
                stopReason = nil
            }

            var deliveries: [ProjectAgentDelivery] = []
            if stopReason == nil {
                for targetAgentID in targets {
                    let triggerKind: ProjectAgentDeliveryTriggerKind = if draft.mentionedAgentIDs.isEmpty {
                        .defaultAgent
                    } else if draft.senderKind == .agent {
                        .agentMention
                    } else {
                        .mention
                    }
                    let deliveryID = UUID().uuidString.lowercased()
                    let deduplicationKey = "\(messageID):\(targetAgentID)"
                    try execute(
                        """
                        INSERT INTO project_agent_deliveries (
                            owner_user_id, id, room_id, message_id, root_message_id,
                            target_agent_id, trigger_kind, status, attempt, hop_count,
                            deduplication_key, response_message_id, last_error,
                            claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms
                        ) VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', 0, ?, ?, NULL, NULL, NULL, NULL, ?)
                        """,
                        [
                            .text(ownerUserID), .text(deliveryID), .text(roomID), .text(messageID),
                            .text(rootMessageID), .text(targetAgentID), .text(triggerKind.rawValue),
                            .integer(Int64(draft.hopCount)), .text(deduplicationKey), .integer(now),
                        ]
                    )
                    guard let delivery = try readDelivery(
                        ownerUserID: ownerUserID,
                        deliveryID: deliveryID
                    ) else { throw AgentGroupChatError.storage("delivery insert was not readable") }
                    deliveries.append(delivery)
                }
            }
            return AgentGroupChatPostResult(
                message: message,
                deliveries: deliveries,
                routingStopReason: stopReason
            )
            }
        } catch {
            if let attachmentDirectoryToRemove {
                try? FileManager.default.removeItem(at: attachmentDirectoryToRemove)
            }
            throw error
        }
    }

    public func messageAttachment(
        ownerUserID: String,
        roomID: String,
        messageID: String,
        attachmentID: String
    ) async throws -> ProjectAgentMessageAttachmentPayload? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(messageID, field: "messageID")
        try AgentGroupChatValidation.identifier(attachmentID, field: "attachmentID")
        guard let stored = try AgentAttachmentRepository.messageAttachment(
            database,
            ownerUserID: ownerUserID,
            messageID: messageID,
            attachmentID: attachmentID,
            roomID: roomID,
            preparedStatement: recordPreparedStatement
        ) else { return nil }
        let fileURL = try attachmentFileURL(relativePath: stored.relativePath)
        if !FileManager.default.fileExists(atPath: fileURL.path) {
            guard stored.attachment.syncStatus == .synced,
                  let artifactID = stored.attachment.artifactID,
                  let expectedSHA256 = stored.attachment.sha256,
                  let agentArtifactService else {
                throw AgentGroupChatError.storage("message attachment file is missing")
            }
            let data = try await agentArtifactService.download(artifactID: artifactID)
            guard data.count == stored.attachment.size,
                  Self.sha256(data) == expectedSHA256 else {
                throw AgentGroupChatError.storage("restored message attachment failed integrity check")
            }
            try FileManager.default.createDirectory(
                at: fileURL.deletingLastPathComponent(),
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: 0o700]
            )
            try data.write(to: fileURL, options: .atomic)
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o600],
                ofItemAtPath: fileURL.path
            )
        }
        return ProjectAgentMessageAttachmentPayload(
            attachment: stored.attachment,
            localFileURL: fileURL
        )
    }

    public func claimNextAgentArtifactUpload(
        ownerUserID: String,
        nowUnixMs: Int64
    ) throws -> ProjectAgentArtifactUploadJob? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        guard let candidate: AgentArtifactUploadCandidate = try transaction({
            guard let candidate = try AgentAttachmentRepository.nextUploadCandidate(
                database,
                ownerUserID: ownerUserID,
                nowUnixMs: nowUnixMs,
                preparedStatement: recordPreparedStatement
            ) else { return nil }
            try execute(
                """
                UPDATE project_agent_message_attachments
                SET sync_status = 'uploading', upload_attempt = ?, upload_error = NULL,
                    next_retry_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ?
                  AND sync_status IN ('queued', 'failed', 'uploading')
                  AND next_retry_at_unix_ms <= ?
                """,
                [
                    .integer(Int64(candidate.attempt)), .integer(nowUnixMs + 300_000),
                    .text(ownerUserID), .text(candidate.id), .integer(nowUnixMs),
                ]
            )
            return sqlite3_changes(database) == 1 ? candidate : nil
        }) else { return nil }

        let fileURL = try attachmentFileURL(relativePath: candidate.relativePath)
        guard let data = try? Data(contentsOf: fileURL, options: [.mappedIfSafe]),
              data.count == candidate.size,
              Self.sha256(data) == candidate.sha256 else {
            try markAgentArtifactUploadFailed(
                ownerUserID: ownerUserID,
                attachmentID: candidate.id,
                attempt: candidate.attempt,
                error: "本地文档缺失或完整性校验失败。",
                nowUnixMs: nowUnixMs
            )
            return nil
        }
        return ProjectAgentArtifactUploadJob(
            attachmentID: candidate.id,
            roomID: candidate.roomID,
            attempt: candidate.attempt,
            request: .init(
                name: candidate.name,
                mimeType: candidate.mimeType,
                data: data,
                sha256: candidate.sha256,
                idempotencyKey: "agent-attachment:\(candidate.id)"
            )
        )
    }

    public func markAgentArtifactUploadSynced(
        ownerUserID: String,
        attachmentID: String,
        metadata: AgentArtifactRemoteMetadata,
        nowUnixMs: Int64
    ) throws {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(attachmentID, field: "attachmentID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        try execute(
            """
            UPDATE project_agent_message_attachments
            SET sync_status = 'synced', artifact_id = ?, storage_provider = ?, bucket = ?,
                object_key = ?, remote_view_path = ?, upload_error = NULL,
                synced_at_unix_ms = ?, next_retry_at_unix_ms = 0
            WHERE owner_user_id = ? AND id = ? AND sync_status = 'uploading'
              AND sha256 = ? AND size_bytes = ?
            """,
            [
                .text(metadata.artifactID), .optionalText(metadata.storageProvider),
                .optionalText(metadata.bucket), .optionalText(metadata.objectKey),
                .optionalText(metadata.remoteViewPath), .integer(nowUnixMs),
                .text(ownerUserID), .text(attachmentID), .text(metadata.sha256),
                .integer(Int64(metadata.size)),
            ]
        )
        guard sqlite3_changes(database) == 1 else { throw AgentGroupChatError.conflict }
    }

    public func markAgentArtifactUploadFailed(
        ownerUserID: String,
        attachmentID: String,
        attempt: Int,
        error: String,
        nowUnixMs: Int64
    ) throws {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(attachmentID, field: "attachmentID")
        guard attempt > 0, nowUnixMs >= 0 else {
            throw AgentGroupChatError.invalidField("artifactUploadFailure")
        }
        let exponent = min(10, attempt - 1)
        let delay = min(Int64(3_600_000), Int64(5_000) * Int64(1 << exponent))
        let safeError = String(error.prefix(512))
        try execute(
            """
            UPDATE project_agent_message_attachments
            SET sync_status = 'failed', upload_error = ?, next_retry_at_unix_ms = ?
            WHERE owner_user_id = ? AND id = ? AND sync_status = 'uploading'
            """,
            [
                .text(safeError), .integer(nowUnixMs + delay),
                .text(ownerUserID), .text(attachmentID),
            ]
        )
    }

    public func retryAgentArtifactUpload(
        ownerUserID: String,
        attachmentID: String
    ) throws {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(attachmentID, field: "attachmentID")
        try execute(
            """
            UPDATE project_agent_message_attachments
            SET sync_status = 'queued', upload_error = NULL, next_retry_at_unix_ms = 0
            WHERE owner_user_id = ? AND id = ? AND sync_status = 'failed'
            """,
            [.text(ownerUserID), .text(attachmentID)]
        )
    }

    public func nextAgentArtifactSyncDue(ownerUserID: String) throws -> Int64? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        return try AgentAttachmentRepository.nextSyncDue(
            database,
            ownerUserID: ownerUserID,
            preparedStatement: recordPreparedStatement
        )
    }

    public func recordAgentMessageAttempt(
        ownerUserID: String,
        characterCount: Int,
        rejected: Bool,
        nowUnixMs: Int64
    ) throws {
        let policy = AgentCommunicationPolicy.standard
        let dimension: String
        switch characterCount {
        case ...policy.conciseMessageCharacters: dimension = "concise"
        case ...policy.recommendedMessageCharacters: dimension = "recommended"
        case ...policy.maximumMessageCharacters: dimension = "extended"
        default: dimension = "over_limit"
        }
        try recordAgentCommunicationMetric(
            ownerUserID: ownerUserID,
            name: "message_length",
            dimension: dimension,
            value: Int64(max(characterCount, 0)),
            nowUnixMs: nowUnixMs
        )
        if rejected {
            try recordAgentToolRejection(
                ownerUserID: ownerUserID,
                reason: .messageTooLong,
                nowUnixMs: nowUnixMs
            )
        }
    }

    public func recordAgentDocumentCreation(
        ownerUserID: String,
        outcome: AgentDocumentCreationMetricOutcome,
        bytes: Int = 0,
        nowUnixMs: Int64
    ) throws {
        try recordAgentCommunicationMetric(
            ownerUserID: ownerUserID,
            name: "document_create",
            dimension: outcome.rawValue,
            value: Int64(max(bytes, 0)),
            nowUnixMs: nowUnixMs
        )
    }

    public func recordAgentArtifactUpload(
        ownerUserID: String,
        outcome: AgentArtifactUploadMetricOutcome,
        bytes: Int,
        nowUnixMs: Int64
    ) throws {
        try recordAgentCommunicationMetric(
            ownerUserID: ownerUserID,
            name: "artifact_upload",
            dimension: outcome.rawValue,
            value: Int64(max(bytes, 0)),
            nowUnixMs: nowUnixMs
        )
    }

    public func recordAgentDocumentPreview(
        ownerUserID: String,
        outcome: AgentDocumentPreviewMetricOutcome,
        durationMilliseconds: Int64,
        nowUnixMs: Int64
    ) throws {
        try recordAgentCommunicationMetric(
            ownerUserID: ownerUserID,
            name: "document_preview",
            dimension: outcome.rawValue,
            value: max(durationMilliseconds, 0),
            nowUnixMs: nowUnixMs
        )
    }

    public func recordAgentToolRejection(
        ownerUserID: String,
        reason: AgentCommunicationRejectionMetricReason,
        nowUnixMs: Int64
    ) throws {
        try recordAgentCommunicationMetric(
            ownerUserID: ownerUserID,
            name: "tool_rejection",
            dimension: reason.rawValue,
            value: 1,
            nowUnixMs: nowUnixMs
        )
    }

    public func agentCommunicationMetricSnapshot(
        ownerUserID: String
    ) throws -> [AgentCommunicationMetricRow] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        return try AgentCommunicationMetricRepository.snapshot(
            database,
            ownerUserID: ownerUserID,
            preparedStatement: recordPreparedStatement
        )
    }

    private func recordAgentCommunicationMetric(
        ownerUserID: String,
        name: String,
        dimension: String,
        value: Int64,
        nowUnixMs: Int64
    ) throws {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        guard value >= 0, nowUnixMs >= 0 else {
            throw AgentGroupChatError.invalidField("communicationMetric")
        }
        try execute(
            """
            INSERT INTO local_agent_communication_metrics (
                owner_user_id, metric_name, dimension, event_count, total_value,
                maximum_value, updated_at_unix_ms
            ) VALUES (?, ?, ?, 1, ?, ?, ?)
            ON CONFLICT(owner_user_id, metric_name, dimension) DO UPDATE SET
                event_count = event_count + 1,
                total_value = total_value + excluded.total_value,
                maximum_value = MAX(maximum_value, excluded.maximum_value),
                updated_at_unix_ms = excluded.updated_at_unix_ms
            """,
            [
                .text(ownerUserID), .text(name), .text(dimension), .integer(value),
                .integer(value), .integer(nowUnixMs),
            ]
        )
    }

}
