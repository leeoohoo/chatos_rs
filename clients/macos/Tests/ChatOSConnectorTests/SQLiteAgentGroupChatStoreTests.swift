@testable import ChatOSConnector
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import SQLite3
import XCTest

final class SQLiteAgentGroupChatStoreTests: XCTestCase {
    struct NoNetworkTicketProvider: LocalConnectorPairingTicketProviding {
        func issueLocalConnectorPairingTicket() async throws -> String {
            throw URLError(.notConnectedToInternet)
        }
    }

    func databaseURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-group-chat-\(UUID().uuidString)")
            .appendingPathComponent("group-chat.db")
    }

    func toolArguments(_ value: [String: Any]) throws -> String {
        let data = try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
        return try XCTUnwrap(String(data: data, encoding: .utf8))
    }

    func executeSQLite(_ databaseURL: URL, sql: String) throws {
        var database: OpaquePointer?
        guard sqlite3_open_v2(
            databaseURL.path,
            &database,
            SQLITE_OPEN_READWRITE | SQLITE_OPEN_FULLMUTEX,
            nil
        ) == SQLITE_OK, let database else {
            defer { sqlite3_close(database) }
            throw AgentGroupChatError.storage("test database open failed")
        }
        defer { sqlite3_close(database) }
        guard sqlite3_exec(database, sql, nil, nil, nil) == SQLITE_OK else {
            throw AgentGroupChatError.storage(String(cString: sqlite3_errmsg(database)))
        }
    }

    func sqliteInt(_ databaseURL: URL, sql: String) throws -> Int64 {
        var database: OpaquePointer?
        guard sqlite3_open_v2(
            databaseURL.path,
            &database,
            SQLITE_OPEN_READONLY | SQLITE_OPEN_FULLMUTEX,
            nil
        ) == SQLITE_OK, let database else {
            defer { sqlite3_close(database) }
            throw AgentGroupChatError.storage("test database open failed")
        }
        defer { sqlite3_close(database) }
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            throw AgentGroupChatError.storage(String(cString: sqlite3_errmsg(database)))
        }
        defer { sqlite3_finalize(statement) }
        guard sqlite3_step(statement) == SQLITE_ROW else {
            throw AgentGroupChatError.storage(String(cString: sqlite3_errmsg(database)))
        }
        return sqlite3_column_int64(statement, 0)
    }

    func sqliteText(_ databaseURL: URL, sql: String) throws -> String {
        var database: OpaquePointer?
        guard sqlite3_open_v2(
            databaseURL.path,
            &database,
            SQLITE_OPEN_READONLY | SQLITE_OPEN_FULLMUTEX,
            nil
        ) == SQLITE_OK, let database else {
            defer { sqlite3_close(database) }
            throw AgentGroupChatError.storage("test database open failed")
        }
        defer { sqlite3_close(database) }
        var statement: OpaquePointer?
        guard sqlite3_prepare_v2(database, sql, -1, &statement, nil) == SQLITE_OK,
              let statement else {
            throw AgentGroupChatError.storage(String(cString: sqlite3_errmsg(database)))
        }
        defer { sqlite3_finalize(statement) }
        guard sqlite3_step(statement) == SQLITE_ROW,
              let value = sqlite3_column_text(statement, 0) else {
            throw AgentGroupChatError.storage(String(cString: sqlite3_errmsg(database)))
        }
        return String(cString: value)
    }

    func testRetiredRequirementSurveyCapabilitiesRemainReadableWithoutMutation() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "兼容旧任务 Agent")
        let room = try await makeRoom(store)
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "legacy-requirement-survey-capabilities",
            draft: .init(title: "读取旧调研任务", teamRoomID: room.id),
            nowUnixMs: 100
        )
        let legacyPlan = """
        {"builtinCapabilities":["requirement_survey_read","requirement_survey_write"],"plugins":[],"requiresExecution":true,"selectedAtUnixMs":99,"selectionRevision":"local-capability-catalog-v1"}
        """
        try executeSQLite(
            url,
            sql: "UPDATE local_agent_todos SET execution_plan_json = '\(legacyPlan)' WHERE id = '\(todo.id)'"
        )

        let loaded = try await store.agentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id
        )

        XCTAssertEqual(loaded?.executionPlan.builtinCapabilities, [])
        XCTAssertEqual(loaded?.executionPlan.selectionRevision, "local-capability-catalog-v1")
        XCTAssertEqual(try sqliteText(
            url,
            sql: "SELECT execution_plan_json FROM local_agent_todos WHERE id = '\(todo.id)'"
        ), legacyPlan)
    }

    func testUnknownPersistedTodoCapabilityStillFailsStrictly() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "未知能力 Agent")
        let room = try await makeRoom(store)
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "unknown-persisted-capability",
            draft: .init(title: "拒绝未知能力", teamRoomID: room.id),
            nowUnixMs: 100
        )
        let unknownPlan = """
        {"builtinCapabilities":["future_untrusted_capability"],"plugins":[],"requiresExecution":true,"selectedAtUnixMs":99,"selectionRevision":"local-capability-catalog-v1"}
        """
        try executeSQLite(
            url,
            sql: "UPDATE local_agent_todos SET execution_plan_json = '\(unknownPlan)' WHERE id = '\(todo.id)'"
        )

        do {
            _ = try await store.agentTodo(
                ownerUserID: "alice",
                agentID: agent.id,
                todoID: todo.id
            )
            XCTFail("unknown persisted capabilities must remain a hard failure")
        } catch let error as AgentGroupChatError {
            XCTAssertEqual(error.localizedDescription, "本地 Agent 群聊存储不可用：invalid Agent Todo execution plan")
        }
    }

    func makeAgent(
        _ store: SQLiteAgentGroupChatStore,
        owner: String = "alice",
        name: String,
        canManageStaff: Bool = false,
        canAccessLocalProjects: Bool = false
    ) async throws -> LocalAgentProfile {
        try await store.createAgent(
            ownerUserID: owner,
            draft: .init(
                name: name,
                rolePrompt: "你是\(name)，只处理当前项目中明确交给你的工作。",
                modelConfigID: "model-1",
                defaultSkillIDs: LocalAgentPermission.normalized(
                    preserving: [],
                    canManageStaff: canManageStaff,
                    canAccessLocalProjects: canAccessLocalProjects
                )
            )
        )
    }

    func makeRoom(
        _ store: SQLiteAgentGroupChatStore,
        owner: String = "alice",
        projectID: String = "project-1"
    ) async throws -> ProjectAgentRoom {
        try await store.createRoom(
            ownerUserID: owner,
            projectID: projectID,
            draft: .init(name: "项目群聊", goal: "协作完成项目")
        )
    }

}
