import ChatOSCore
import SQLite3

struct AgentGroupChatMigrationDatabase {
    let handle: OpaquePointer

    func hasColumn(_ name: String, table: String) -> Bool {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(handle, "PRAGMA table_info(\(table))", -1, &statement, nil) == SQLITE_OK,
              let statement else { return false }
        defer { sqlite3_finalize(statement) }
        while sqlite3_step(statement) == SQLITE_ROW {
            guard let value = sqlite3_column_text(statement, 1) else { continue }
            if String(cString: value) == name { return true }
        }
        return false
    }

    func execute(_ sql: String) throws {
        guard sqlite3_exec(handle, sql, nil, nil, nil) == SQLITE_OK else {
            throw AgentGroupChatError.storage(String(cString: sqlite3_errmsg(handle)))
        }
    }

    func hasMigration(_ version: Int) -> Bool {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(
            handle,
            "SELECT 1 FROM local_agent_group_chat_schema_migrations WHERE version = ? LIMIT 1",
            -1,
            &statement,
            nil
        ) == SQLITE_OK, let statement else { return false }
        defer { sqlite3_finalize(statement) }
        guard sqlite3_bind_int64(statement, 1, Int64(version)) == SQLITE_OK else { return false }
        return sqlite3_step(statement) == SQLITE_ROW
    }
}
