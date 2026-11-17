import ChatOSCore
import Foundation
import SQLite3

extension SQLiteAgentGroupChatStore {
    public func listTeamAssetReviews(
        ownerUserID: String,
        teamRoomID: String?,
        reviewerAgentID: String?,
        status: LocalAgentTeamAssetReviewStatus?,
        limit: Int = 500
    ) throws -> [LocalAgentTeamAssetReview] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        if let teamRoomID {
            try AgentGroupChatValidation.identifier(teamRoomID, field: "teamRoomID")
        }
        if let reviewerAgentID {
            try AgentGroupChatValidation.identifier(reviewerAgentID, field: "reviewerAgentID")
        }
        guard (1...500).contains(limit) else {
            throw AgentGroupChatError.invalidField("teamAssetReviewLimit")
        }
        return try AgentTeamAssetReviewRepository.list(
            database,
            ownerUserID: ownerUserID,
            teamRoomID: teamRoomID,
            reviewerAgentID: reviewerAgentID,
            status: status,
            limit: limit,
            preparedStatement: recordPreparedStatement
        )
    }

    public func teamAssetReview(
        ownerUserID: String,
        reviewID: String
    ) throws -> LocalAgentTeamAssetReview? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(reviewID, field: "teamAssetReviewID")
        return try AgentTeamAssetReviewRepository.find(
            database,
            ownerUserID: ownerUserID,
            reviewID: reviewID,
            preparedStatement: recordPreparedStatement
        )
    }

    public func resolveTeamAssetReview(
        ownerUserID: String,
        reviewID: String,
        reviewerAgentID: String,
        status: LocalAgentTeamAssetReviewStatus,
        resolvedAssetID: String?,
        resolvedAssetRevision: Int?,
        reason: String,
        nowUnixMs: Int64
    ) throws -> LocalAgentTeamAssetReview {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(reviewID, field: "teamAssetReviewID")
        try AgentGroupChatValidation.identifier(reviewerAgentID, field: "reviewerAgentID")
        try AgentGroupChatValidation.optionalText(
            reason,
            field: "teamAssetReviewReason",
            maximumLength: 4_000
        )
        guard status != .pending, nowUnixMs >= 0 else {
            throw AgentGroupChatError.invalidField("teamAssetReviewStatus")
        }
        return try transaction {
            guard let review = try AgentTeamAssetReviewRepository.find(
                database,
                ownerUserID: ownerUserID,
                reviewID: reviewID,
                preparedStatement: recordPreparedStatement
            ) else {
                throw AgentGroupChatError.notFound
            }
            if review.status != .pending {
                guard review.status == status,
                      review.reviewedByAgentID == reviewerAgentID,
                      review.resolvedAssetID == resolvedAssetID,
                      review.resolvedAssetRevision == resolvedAssetRevision,
                      review.reviewReason == reason else {
                    throw AgentGroupChatError.conflict
                }
                return review
            }
            guard let team = try readRoom(
                ownerUserID: ownerUserID,
                roomID: review.teamRoomID
            ), team.status == .active,
            team.projectManagerAgentID == reviewerAgentID else {
                throw AgentGroupChatError.permissionDenied
            }

            let assetID: String?
            let assetRevision: Int?
            switch status {
            case .applied:
                guard let resolvedAssetID, let resolvedAssetRevision,
                      let asset = try AgentTeamAssetRepository.find(
                        database,
                        ownerUserID: ownerUserID,
                        teamRoomID: review.teamRoomID,
                        assetID: resolvedAssetID,
                        preparedStatement: recordPreparedStatement
                      ), asset.status == .active,
                      asset.revision == resolvedAssetRevision else {
                    throw AgentGroupChatError.conflict
                }
                assetID = asset.id
                assetRevision = asset.revision
            case .rejected:
                guard !reason.isEmpty else {
                    throw AgentGroupChatError.invalidField("teamAssetReviewReason")
                }
                assetID = nil
                assetRevision = nil
            case .pending:
                throw AgentGroupChatError.invalidField("teamAssetReviewStatus")
            }

            try execute(
                """
                UPDATE local_agent_team_asset_reviews
                SET status = ?, resolved_asset_id = ?, resolved_asset_revision = ?,
                    reviewed_by_agent_id = ?, review_reason = ?, updated_at_unix_ms = ?
                WHERE owner_user_id = ? AND id = ? AND status = 'pending'
                """,
                [
                    .text(status.rawValue), .optionalText(assetID),
                    assetRevision.map { .integer(Int64($0)) } ?? .null,
                    .text(reviewerAgentID), .text(reason), .integer(nowUnixMs),
                    .text(ownerUserID), .text(reviewID),
                ]
            )
            guard sqlite3_changes(database) == 1,
                  let updated = try AgentTeamAssetReviewRepository.find(
                    database,
                    ownerUserID: ownerUserID,
                    reviewID: reviewID,
                    preparedStatement: recordPreparedStatement
                  ) else {
                throw AgentGroupChatError.conflict
            }
            return updated
        }
    }
}
