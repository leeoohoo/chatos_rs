import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import SQLite3

extension SQLiteAgentGroupChatStore {
    typealias Value = AgentGroupChatDatabase.Value

    func query<T>(
        _ sql: String,
        _ values: [Value] = [],
        row: (OpaquePointer) throws -> T
    ) throws -> [T] {
        recordPreparedStatement()
        return try AgentGroupChatDatabase.query(database, sql, values, row: row)
    }

    func execute(_ sql: String, _ values: [Value] = []) throws {
        recordPreparedStatement()
        try AgentGroupChatDatabase.execute(database, sql, values)
    }

    func scalarInt64(_ sql: String, _ values: [Value]) throws -> Int64 {
        recordPreparedStatement()
        return try AgentGroupChatDatabase.scalarInt64(database, sql, values)
    }

    func transaction<T>(_ body: () throws -> T) throws -> T {
        try AgentGroupChatDatabase.transaction(
            database,
            preparedStatement: recordPreparedStatement,
            body: body
        )
    }

    func recordPreparedStatement() {
#if DEBUG
        debugPreparedStatementCount += 1
#endif
    }

    func encodeStrings(_ values: [String]) throws -> String {
        String(decoding: try JSONEncoder().encode(values), as: UTF8.self)
    }

    func encodeJSON<Value: Encodable>(_ value: Value) throws -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        return String(decoding: try encoder.encode(value), as: UTF8.self)
    }

    static func string(_ statement: OpaquePointer, _ index: Int32) -> String {
        guard let value = sqlite3_column_text(statement, index) else { return "" }
        return String(cString: value)
    }

    static func optionalString(_ statement: OpaquePointer, _ index: Int32) -> String? {
        guard sqlite3_column_type(statement, index) != SQLITE_NULL,
              let value = sqlite3_column_text(statement, index) else { return nil }
        return String(cString: value)
    }

    static func optionalInt64(_ statement: OpaquePointer, _ index: Int32) -> Int64? {
        sqlite3_column_type(statement, index) == SQLITE_NULL
            ? nil
            : sqlite3_column_int64(statement, index)
    }

    static func now() -> Int64 { Int64(Date().timeIntervalSince1970 * 1_000) }

    static let messageColumns = "owner_user_id, id, room_id, sender_kind, sender_id, content, reply_to_message_id, source_run_id, causation_id, root_message_id, hop_count, created_at_unix_ms"
    static let todoColumns = "owner_user_id, id, agent_id, team_room_id, source_room_id, source_message_id, title, detail, priority, sort_order, request_key, status, blocked_reason, result, created_at_unix_ms, updated_at_unix_ms, execution_plan_json, execution_contract_json"
}
