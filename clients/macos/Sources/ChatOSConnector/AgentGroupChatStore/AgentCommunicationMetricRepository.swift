import SQLite3

enum AgentCommunicationMetricRepository {
    static func snapshot(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        preparedStatement: () -> Void
    ) throws -> [AgentCommunicationMetricRow] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT metric_name, dimension, event_count, total_value, maximum_value,
                   updated_at_unix_ms
            FROM local_agent_communication_metrics
            WHERE owner_user_id = ?
            ORDER BY metric_name, dimension
            """,
            [.text(ownerUserID)]
        ) { statement in
            AgentCommunicationMetricRow(
                name: string(statement, 0),
                dimension: string(statement, 1),
                count: sqlite3_column_int64(statement, 2),
                totalValue: sqlite3_column_int64(statement, 3),
                maximumValue: sqlite3_column_int64(statement, 4),
                updatedAtUnixMs: sqlite3_column_int64(statement, 5)
            )
        }
    }

    private static func string(_ statement: OpaquePointer, _ index: Int32) -> String {
        guard let value = sqlite3_column_text(statement, index) else { return "" }
        return String(cString: value)
    }
}
