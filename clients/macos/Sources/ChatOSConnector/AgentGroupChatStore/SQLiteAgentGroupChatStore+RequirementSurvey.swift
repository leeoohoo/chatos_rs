import ChatOSCore
import Foundation
import SQLite3

extension SQLiteAgentGroupChatStore {
    public func createRequirementSurvey(
        ownerUserID: String,
        projectID: String,
        creatorAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentRequirementSurveyDraft,
        nowUnixMs: Int64
    ) throws -> LocalAgentRequirementSurvey {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(projectID, field: "projectID")
        try AgentGroupChatValidation.identifier(creatorAgentID, field: "creatorAgentID")
        try AgentGroupChatValidation.identifier(sourceDeliveryID, field: "sourceDeliveryID")
        try AgentGroupChatValidation.identifier(requestKey, field: "requestKey")
        try draft.validate()
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            // Identity, project and write authority are fixed by the task capability runtime.
            // This store deliberately does not reintroduce Agent-profession or profile-Skill
            // authorization; it also accepts Task Runner provenance without an Agent/Delivery row.
            if let existing = try AgentRequirementSurveyRepository.findByRequest(
                database,
                ownerUserID: ownerUserID,
                projectID: projectID,
                creatorAgentID: creatorAgentID,
                sourceDeliveryID: sourceDeliveryID,
                requestKey: requestKey,
                preparedStatement: recordPreparedStatement
            ) {
                guard existing.draft == draft else { throw AgentGroupChatError.conflict }
                return existing
            }
            let survey = LocalAgentRequirementSurvey(
                id: UUID().uuidString.lowercased(),
                ownerUserID: ownerUserID,
                projectID: projectID,
                creatorAgentID: creatorAgentID,
                sourceDeliveryID: sourceDeliveryID,
                requestKey: requestKey,
                draft: draft,
                createdAtUnixMs: nowUnixMs
            )
            try survey.validate()
            try execute(
                """
                INSERT INTO local_agent_requirement_surveys (
                    owner_user_id, id, project_id, creator_agent_id,
                    source_delivery_id, request_key, draft_json, status,
                    submission_json, created_at_unix_ms, submitted_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', NULL, ?, NULL)
                """,
                [
                    .text(ownerUserID), .text(survey.id), .text(projectID),
                    .text(creatorAgentID), .text(sourceDeliveryID), .text(requestKey),
                    .text(try encodeJSON(draft)), .integer(nowUnixMs),
                ]
            )
            return survey
        }
    }

    public func listRequirementSurveys(
        ownerUserID: String,
        projectID: String,
        status: LocalAgentRequirementSurveyStatus? = nil
    ) throws -> [LocalAgentRequirementSurvey] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(projectID, field: "projectID")
        return try AgentRequirementSurveyRepository.list(
            database,
            ownerUserID: ownerUserID,
            projectID: projectID,
            status: status,
            preparedStatement: recordPreparedStatement
        )
    }

    public func requirementSurvey(
        ownerUserID: String,
        projectID: String,
        surveyID: String
    ) throws -> LocalAgentRequirementSurvey? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(projectID, field: "projectID")
        try AgentGroupChatValidation.identifier(surveyID, field: "requirementSurveyID")
        return try AgentRequirementSurveyRepository.find(
            database,
            ownerUserID: ownerUserID,
            projectID: projectID,
            surveyID: surveyID,
            preparedStatement: recordPreparedStatement
        )
    }

    public func submitRequirementSurvey(
        ownerUserID: String,
        projectID: String,
        surveyID: String,
        submission: LocalAgentRequirementSurveySubmission,
        nowUnixMs: Int64
    ) throws -> LocalAgentRequirementSurvey {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(projectID, field: "projectID")
        try AgentGroupChatValidation.identifier(surveyID, field: "requirementSurveyID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard let existing = try AgentRequirementSurveyRepository.find(
                database,
                ownerUserID: ownerUserID,
                projectID: projectID,
                surveyID: surveyID,
                preparedStatement: recordPreparedStatement
            ) else { throw AgentGroupChatError.notFound }
            try LocalAgentRequirementSurvey.validate(
                submission: submission,
                questions: existing.draft.questions
            )
            if existing.status == .submitted {
                guard existing.submission == submission else { throw AgentGroupChatError.conflict }
                return existing
            }
            let notificationTarget: (roomID: String, agentID: String)?
            if let room = try readActiveRoom(ownerUserID: ownerUserID, projectID: projectID),
               room.status == .active,
               let managerID = room.projectManagerAgentID,
               try readMember(
                    ownerUserID: ownerUserID,
                    roomID: room.id,
                    agentID: managerID
               )?.status == .active {
                    notificationTarget = (room.id, managerID)
            } else {
                notificationTarget = nil
            }
            let submittedAt = max(nowUnixMs, existing.createdAtUnixMs)
            try execute(
                """
                UPDATE local_agent_requirement_surveys
                SET status = 'submitted', submission_json = ?, submitted_at_unix_ms = ?
                WHERE owner_user_id = ? AND project_id = ? AND id = ? AND status = 'pending'
                """,
                [
                    .text(try encodeJSON(submission)), .integer(submittedAt),
                    .text(ownerUserID), .text(projectID), .text(surveyID),
                ]
            )
            guard sqlite3_changes(database) == 1 else { throw AgentGroupChatError.conflict }
            if let notificationTarget {
                try enqueueRequirementSurveySubmittedNotification(
                    ownerUserID: ownerUserID,
                    roomID: notificationTarget.roomID,
                    managerAgentID: notificationTarget.agentID,
                    surveyID: surveyID,
                    title: existing.draft.title,
                    nowUnixMs: submittedAt
                )
            }
            guard let submitted = try AgentRequirementSurveyRepository.find(
                database,
                ownerUserID: ownerUserID,
                projectID: projectID,
                surveyID: surveyID,
                preparedStatement: recordPreparedStatement
            ) else { throw AgentGroupChatError.storage("submitted survey disappeared") }
            return submitted
        }
    }

    public func resolveRequirementSurvey(
        ownerUserID: String,
        projectID: String,
        surveyID: String,
        resolverAgentID: String,
        resolution: LocalAgentRequirementSurveyResolution,
        nowUnixMs: Int64
    ) throws -> LocalAgentRequirementSurvey {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(projectID, field: "projectID")
        try AgentGroupChatValidation.identifier(resolverAgentID, field: "resolverAgentID")
        try AgentGroupChatValidation.identifier(surveyID, field: "requirementSurveyID")
        try resolution.validate()
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard let survey = try AgentRequirementSurveyRepository.find(
                database,
                ownerUserID: ownerUserID,
                projectID: projectID,
                surveyID: surveyID,
                preparedStatement: recordPreparedStatement
            ) else { throw AgentGroupChatError.notFound }
            guard survey.status == .submitted,
                  let submittedAtUnixMs = survey.submittedAtUnixMs else {
                throw AgentGroupChatError.conflict
            }
            if survey.resolution == resolution { return survey }
            let resolvedAt = max(nowUnixMs, submittedAtUnixMs)
            try execute(
                """
                UPDATE local_agent_requirement_surveys
                SET resolution_json = ?, resolved_at_unix_ms = ?
                WHERE owner_user_id = ? AND project_id = ? AND id = ?
                  AND status = 'submitted'
                """,
                [
                    .text(try encodeJSON(resolution)), .integer(resolvedAt),
                    .text(ownerUserID), .text(projectID), .text(surveyID),
                ]
            )
            guard sqlite3_changes(database) == 1,
                  let resolved = try AgentRequirementSurveyRepository.find(
                    database,
                    ownerUserID: ownerUserID,
                    projectID: projectID,
                    surveyID: surveyID,
                    preparedStatement: recordPreparedStatement
                  ) else { throw AgentGroupChatError.conflict }
            return resolved
        }
    }

    private func enqueueRequirementSurveySubmittedNotification(
        ownerUserID: String,
        roomID: String,
        managerAgentID: String,
        surveyID: String,
        title: String,
        nowUnixMs: Int64
    ) throws {
        let messageID = UUID().uuidString.lowercased()
        let deduplicationKey = "requirement-survey-submitted:\(surveyID)"
        try execute(
            """
            INSERT INTO project_agent_messages (
                owner_user_id, id, room_id, sender_kind, sender_id, content,
                reply_to_message_id, source_run_id, causation_id, root_message_id,
                hop_count, created_at_unix_ms
            ) VALUES (?, ?, ?, 'system', 'system', ?, NULL, NULL, ?, ?, 0, ?)
            """,
            [
                .text(ownerUserID), .text(messageID), .text(roomID),
                .text("Human 已提交需求调研《\(title)》。请创建一个任务来读取并处理这张调研：为任务选择“需求调研创建与方案”基础能力，程序会自动同时加入“需求调研读取”。执行层必须先 list/get 读取真实答案和备注，再把解决方案与执行计划写回原调研单；需要继续实施时再创建对应 Todo。"),
                .text(deduplicationKey), .text(messageID), .integer(nowUnixMs),
            ]
        )
        try execute(
            """
            INSERT INTO project_agent_message_mentions (
                owner_user_id, message_id, agent_id, position
            ) VALUES (?, ?, ?, 0)
            """,
            [.text(ownerUserID), .text(messageID), .text(managerAgentID)]
        )
        try execute(
            """
            INSERT INTO project_agent_deliveries (
                owner_user_id, id, room_id, message_id, root_message_id,
                target_agent_id, trigger_kind, status, attempt, hop_count,
                deduplication_key, response_message_id, last_error,
                claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms
            ) VALUES (?, ?, ?, ?, ?, ?, 'mention', 'pending', 0, 0, ?, NULL, NULL, NULL, NULL, ?)
            """,
            [
                .text(ownerUserID), .text(UUID().uuidString.lowercased()),
                .text(roomID), .text(messageID), .text(messageID),
                .text(managerAgentID), .text(deduplicationKey), .integer(nowUnixMs),
            ]
        )
    }
}
