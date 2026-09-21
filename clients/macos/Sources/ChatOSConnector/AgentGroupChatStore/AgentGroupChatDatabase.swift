import ChatOSCore
import Foundation
import SQLite3

enum AgentGroupChatDatabase {
    enum Value {
        case text(String)
        case integer(Int64)
        case blob(Data)
        case null

        static func optionalText(_ value: String?) -> Self { value.map(Self.text) ?? .null }
        static func optionalBlob(_ value: Data?) -> Self { value.map(Self.blob) ?? .null }
    }

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

    static func query<T>(
        _ handle: OpaquePointer?,
        _ sql: String,
        _ values: [Value] = [],
        row: (OpaquePointer) throws -> T
    ) throws -> [T] {
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(handle, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else { throw storageError(handle) }
        defer { sqlite3_finalize(statement) }
        for (offset, value) in values.enumerated() {
            let index = Int32(offset + 1)
            let result: Int32
            switch value {
            case let .text(text):
                result = sqlite3_bind_text(
                    statement,
                    index,
                    text,
                    -1,
                    unsafeBitCast(-1, to: sqlite3_destructor_type.self)
                )
            case let .integer(number):
                result = sqlite3_bind_int64(statement, index, number)
            case let .blob(data):
                result = data.withUnsafeBytes { bytes in
                    sqlite3_bind_blob(
                        statement,
                        index,
                        bytes.baseAddress,
                        Int32(bytes.count),
                        unsafeBitCast(-1, to: sqlite3_destructor_type.self)
                    )
                }
            case .null:
                result = sqlite3_bind_null(statement, index)
            }
            guard result == SQLITE_OK else { throw storageError(handle) }
        }
        var rows: [T] = []
        while true {
            switch sqlite3_step(statement) {
            case SQLITE_ROW: rows.append(try row(statement))
            case SQLITE_DONE: return rows
            default: throw storageError(handle)
            }
        }
    }

    static func execute(
        _ handle: OpaquePointer?,
        _ sql: String,
        _ values: [Value] = []
    ) throws {
        let _: [Int] = try query(handle, sql, values) { _ in 0 }
    }

    static func scalarInt64(
        _ handle: OpaquePointer?,
        _ sql: String,
        _ values: [Value]
    ) throws -> Int64 {
        try query(handle, sql, values) { sqlite3_column_int64($0, 0) }.first ?? 0
    }

    static func transaction<T>(
        _ handle: OpaquePointer?,
        preparedStatement: () -> Void,
        body: () throws -> T
    ) throws -> T {
        preparedStatement()
        try execute(handle, "BEGIN IMMEDIATE")
        do {
            let result = try body()
            preparedStatement()
            try execute(handle, "COMMIT")
            return result
        } catch {
            preparedStatement()
            try? execute(handle, "ROLLBACK")
            throw error
        }
    }

    private static func storageError(_ handle: OpaquePointer?) -> AgentGroupChatError {
        .storage(String(cString: sqlite3_errmsg(handle)))
    }
}
