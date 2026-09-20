import ChatOSCore
import SQLite3

extension SQLiteAgentGroupChatStore {
    public func recordAgentMessageSent(
        ownerUserID: String,
        runFingerprint: String,
        characterCount: Int,
        documentCount: Int,
        nowUnixMs: Int64
    ) throws {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.text(
            runFingerprint,
            field: "runFingerprint",
            maximumLength: 64
        )
        guard characterCount >= 0, documentCount >= 0, nowUnixMs >= 0 else {
            throw AgentGroupChatError.invalidField("messageSequence")
        }
        let observationWindowMilliseconds: Int64 = 5 * 60 * 1_000
        let retentionMilliseconds: Int64 = 24 * 60 * 60 * 1_000
        let windowStart = max(0, nowUnixMs - observationWindowMilliseconds)
        let retentionStart = max(0, nowUnixMs - retentionMilliseconds)
        try transaction {
            try execute(
                """
                DELETE FROM local_agent_message_sequence_events
                WHERE owner_user_id = ? AND created_at_unix_ms < ?
                """,
                [.text(ownerUserID), .integer(retentionStart)]
            )
            let prior = try query(
                """
                SELECT COUNT(*), COALESCE(SUM(character_count), 0),
                       COALESCE(SUM(document_count), 0)
                FROM local_agent_message_sequence_events
                WHERE owner_user_id = ? AND run_fingerprint = ?
                  AND created_at_unix_ms >= ?
                """,
                [.text(ownerUserID), .text(runFingerprint), .integer(windowStart)]
            ) { statement in
                (
                    count: sqlite3_column_int64(statement, 0),
                    characters: sqlite3_column_int64(statement, 1),
                    documents: sqlite3_column_int64(statement, 2)
                )
            }.first ?? (count: 0, characters: 0, documents: 0)
            let previousSequence = try query(
                """
                SELECT COALESCE(MAX(sequence), 0)
                FROM local_agent_message_sequence_events
                WHERE owner_user_id = ? AND run_fingerprint = ?
                """,
                [.text(ownerUserID), .text(runFingerprint)]
            ) { statement in
                sqlite3_column_int64(statement, 0)
            }.first ?? 0
            try execute(
                """
                INSERT INTO local_agent_message_sequence_events (
                    owner_user_id, run_fingerprint, sequence, character_count,
                    document_count, created_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?)
                """,
                [
                    .text(ownerUserID), .text(runFingerprint), .integer(previousSequence + 1),
                    .integer(Int64(characterCount)), .integer(Int64(documentCount)),
                    .integer(nowUnixMs),
                ]
            )
            let policy = AgentCommunicationPolicy.standard
            let crossedLimit = prior.count > 0
                && prior.characters <= Int64(policy.maximumMessageCharacters)
                && prior.characters + Int64(characterCount)
                    > Int64(policy.maximumMessageCharacters)
            if crossedLimit, prior.documents + Int64(documentCount) == 0 {
                try recordAgentCommunicationMetric(
                    ownerUserID: ownerUserID,
                    name: "message_sequence",
                    dimension: "possible_limit_bypass",
                    value: prior.count + 1,
                    nowUnixMs: nowUnixMs
                )
            }
        }
    }
}
