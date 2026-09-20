import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import SQLite3

extension SQLiteAgentGroupChatStore {
    public func listTeamAssets(
        ownerUserID: String,
        teamRoomID: String,
        includeArchived: Bool = false
    ) throws -> [LocalAgentTeamAsset] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(teamRoomID, field: "teamRoomID")
        guard try readRoom(ownerUserID: ownerUserID, roomID: teamRoomID)?.conversationKind
            == .projectTeam else { throw AgentGroupChatError.notFound }
        return try AgentTeamAssetRepository.list(
            database,
            ownerUserID: ownerUserID,
            teamRoomID: teamRoomID,
            includeArchived: includeArchived,
            preparedStatement: recordPreparedStatement
        )
    }

    public func teamAsset(
        ownerUserID: String,
        teamRoomID: String,
        assetID: String
    ) throws -> LocalAgentTeamAsset? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(teamRoomID, field: "teamRoomID")
        try AgentGroupChatValidation.identifier(assetID, field: "teamAssetID")
        return try AgentTeamAssetRepository.find(
            database,
            ownerUserID: ownerUserID,
            teamRoomID: teamRoomID,
            assetID: assetID,
            preparedStatement: recordPreparedStatement
        )
    }

    public func upsertTeamAsset(
        ownerUserID: String,
        teamRoomID: String,
        assetID: String?,
        editorAgentID: String?,
        category: LocalAgentTeamAssetCategory,
        title: String,
        markdown: String,
        expectedRevision: Int?,
        nowUnixMs: Int64
    ) throws -> LocalAgentTeamAsset {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(teamRoomID, field: "teamRoomID")
        if let assetID { try AgentGroupChatValidation.identifier(assetID, field: "teamAssetID") }
        if let editorAgentID {
            try AgentGroupChatValidation.identifier(editorAgentID, field: "editorAgentID")
        }
        try AgentGroupChatValidation.text(title, field: "teamAssetTitle", maximumLength: 240)
        try AgentGroupChatValidation.optionalText(
            markdown,
            field: "teamAssetMarkdown",
            maximumLength: 128_000
        )
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard let team = try readRoom(ownerUserID: ownerUserID, roomID: teamRoomID),
                  team.status == .active, team.conversationKind == .projectTeam else {
                throw AgentGroupChatError.notFound
            }
            if let editorAgentID {
                guard team.projectManagerAgentID == editorAgentID,
                      try readMember(
                        ownerUserID: ownerUserID,
                        roomID: teamRoomID,
                        agentID: editorAgentID
                      )?.status == .active else {
                    throw AgentGroupChatError.permissionDenied
                }
            }
            let resolvedID = assetID ?? UUID().uuidString.lowercased()
            let existing = try AgentTeamAssetRepository.find(
                database,
                ownerUserID: ownerUserID,
                teamRoomID: teamRoomID,
                assetID: resolvedID,
                preparedStatement: recordPreparedStatement
            )
            let asset: LocalAgentTeamAsset
            if let existing {
                guard existing.status == .active,
                      let expectedRevision,
                      expectedRevision == existing.revision else {
                    throw AgentGroupChatError.conflict
                }
                asset = .init(
                    id: existing.id,
                    ownerUserID: existing.ownerUserID,
                    teamRoomID: existing.teamRoomID,
                    category: category,
                    title: title,
                    markdown: markdown,
                    revision: existing.revision + 1,
                    status: .active,
                    createdByAgentID: existing.createdByAgentID,
                    updatedByAgentID: editorAgentID,
                    createdAtUnixMs: existing.createdAtUnixMs,
                    updatedAtUnixMs: max(nowUnixMs, existing.createdAtUnixMs)
                )
                try execute(
                    """
                    UPDATE local_agent_team_assets
                    SET category = ?, title = ?, markdown = ?, revision = ?,
                        updated_by_agent_id = ?, updated_at_unix_ms = ?
                    WHERE owner_user_id = ? AND team_room_id = ? AND id = ?
                      AND status = 'active' AND revision = ?
                    """,
                    [
                        .text(category.rawValue), .text(title), .text(markdown),
                        .integer(Int64(asset.revision)), .optionalText(editorAgentID),
                        .integer(asset.updatedAtUnixMs), .text(ownerUserID), .text(teamRoomID),
                        .text(resolvedID), .integer(Int64(existing.revision)),
                    ]
                )
                guard sqlite3_changes(database) == 1 else { throw AgentGroupChatError.conflict }
            } else {
                guard assetID == nil, expectedRevision == nil else {
                    throw AgentGroupChatError.notFound
                }
                asset = .init(
                    id: resolvedID,
                    ownerUserID: ownerUserID,
                    teamRoomID: teamRoomID,
                    category: category,
                    title: title,
                    markdown: markdown,
                    revision: 1,
                    createdByAgentID: editorAgentID,
                    updatedByAgentID: editorAgentID,
                    createdAtUnixMs: nowUnixMs,
                    updatedAtUnixMs: nowUnixMs
                )
                try execute(
                    """
                    INSERT INTO local_agent_team_assets (
                        owner_user_id, id, team_room_id, category, title, markdown,
                        revision, status, created_by_agent_id, updated_by_agent_id,
                        created_at_unix_ms, updated_at_unix_ms
                    ) VALUES (?, ?, ?, ?, ?, ?, 1, 'active', ?, ?, ?, ?)
                    """,
                    [
                        .text(ownerUserID), .text(resolvedID), .text(teamRoomID),
                        .text(category.rawValue), .text(title), .text(markdown),
                        .optionalText(editorAgentID), .optionalText(editorAgentID),
                        .integer(nowUnixMs), .integer(nowUnixMs),
                    ]
                )
            }
            try asset.validate()
            try execute(
                """
                INSERT INTO local_agent_team_asset_revisions (
                    owner_user_id, asset_id, revision, title, markdown,
                    editor_agent_id, created_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?)
                """,
                [
                    .text(ownerUserID), .text(asset.id), .integer(Int64(asset.revision)),
                    .text(asset.title), .text(asset.markdown),
                    .optionalText(editorAgentID), .integer(asset.updatedAtUnixMs),
                ]
            )
            return asset
        }
    }

    public func archiveTeamAsset(
        ownerUserID: String,
        teamRoomID: String,
        assetID: String,
        editorAgentID: String?,
        expectedRevision: Int,
        nowUnixMs: Int64
    ) throws -> LocalAgentTeamAsset {
        guard let existing = try teamAsset(
            ownerUserID: ownerUserID,
            teamRoomID: teamRoomID,
            assetID: assetID
        ), existing.status == .active, existing.revision == expectedRevision else {
            throw AgentGroupChatError.conflict
        }
        if let editorAgentID {
            guard try readRoom(
                ownerUserID: ownerUserID,
                roomID: teamRoomID
            )?.projectManagerAgentID == editorAgentID else {
                throw AgentGroupChatError.permissionDenied
            }
        }
        return try transaction {
            try execute(
                """
                UPDATE local_agent_team_assets
                SET status = 'archived', revision = revision + 1,
                    updated_by_agent_id = ?, updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND team_room_id = ? AND id = ?
                  AND status = 'active' AND revision = ?
                """,
                [
                    .optionalText(editorAgentID), .integer(nowUnixMs), .text(ownerUserID),
                    .text(teamRoomID), .text(assetID), .integer(Int64(expectedRevision)),
                ]
            )
            guard sqlite3_changes(database) == 1,
                  let archived = try teamAsset(
                    ownerUserID: ownerUserID,
                    teamRoomID: teamRoomID,
                    assetID: assetID
                  ) else { throw AgentGroupChatError.conflict }
            try execute(
                """
                INSERT INTO local_agent_team_asset_revisions (
                    owner_user_id, asset_id, revision, title, markdown,
                    editor_agent_id, created_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?)
                """,
                [
                    .text(ownerUserID), .text(assetID), .integer(Int64(archived.revision)),
                    .text(archived.title), .text(archived.markdown),
                    .optionalText(editorAgentID), .integer(nowUnixMs),
                ]
            )
            return archived
        }
    }

    public func listTeamAssetRevisions(
        ownerUserID: String,
        teamRoomID: String,
        assetID: String,
        limit: Int = 100
    ) throws -> [LocalAgentTeamAssetRevision] {
        guard try teamAsset(
            ownerUserID: ownerUserID,
            teamRoomID: teamRoomID,
            assetID: assetID
        ) != nil, (1...500).contains(limit) else {
            throw AgentGroupChatError.notFound
        }
        return try AgentTeamAssetRepository.listRevisions(
            database,
            ownerUserID: ownerUserID,
            assetID: assetID,
            limit: limit,
            preparedStatement: recordPreparedStatement
        )
    }

    public func listTodoTeamAssetSnapshots(
        ownerUserID: String,
        todoID: String
    ) throws -> [LocalAgentTodoTeamAssetSnapshot] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        return try AgentTeamAssetRepository.listTodoSnapshots(
            database,
            ownerUserID: ownerUserID,
            todoID: todoID,
            preparedStatement: recordPreparedStatement
        )
    }

    public func todoTeamAssetSnapshot(
        ownerUserID: String,
        todoID: String,
        assetID: String,
        revision: Int
    ) throws -> LocalAgentTodoTeamAssetSnapshot? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        try AgentGroupChatValidation.identifier(assetID, field: "teamAssetID")
        guard revision > 0 else { throw AgentGroupChatError.invalidField("teamAssetRevision") }
        return try AgentTeamAssetRepository.todoSnapshot(
            database,
            ownerUserID: ownerUserID,
            todoID: todoID,
            assetID: assetID,
            revision: revision,
            preparedStatement: recordPreparedStatement
        )
    }

}
