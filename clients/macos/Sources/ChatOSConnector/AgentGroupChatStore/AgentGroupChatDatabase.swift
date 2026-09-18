import ChatOSCore
import Foundation
import SQLite3

enum AgentGroupChatDatabase {
    static func open(at databaseURL: URL) throws -> OpaquePointer? {
        try FileManager.default.createDirectory(
            at: databaseURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        var handle: OpaquePointer?
        guard sqlite3_open_v2(
            databaseURL.path,
            &handle,
            SQLITE_OPEN_CREATE | SQLITE_OPEN_READWRITE | SQLITE_OPEN_FULLMUTEX,
            nil
        ) == SQLITE_OK else {
            let message = handle.map { String(cString: sqlite3_errmsg($0)) } ?? "open failed"
            sqlite3_close(handle)
            throw AgentGroupChatError.storage(message)
        }
        do {
            sqlite3_busy_timeout(handle, 5_000)
            guard sqlite3_exec(
                handle,
                AgentGroupChatSchema.definition,
                nil,
                nil,
                nil
            ) == SQLITE_OK else {
                throw AgentGroupChatError.storage(String(cString: sqlite3_errmsg(handle)))
            }
            try AgentGroupChatMigrations.migrateConversationSchema(handle)
            return handle
        } catch {
            sqlite3_close(handle)
            throw error
        }
    }

    static func close(_ handle: OpaquePointer?) {
        sqlite3_close(handle)
    }
}
