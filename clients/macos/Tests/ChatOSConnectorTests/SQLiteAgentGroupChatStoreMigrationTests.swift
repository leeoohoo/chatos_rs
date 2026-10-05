@testable import ChatOSConnector
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import SQLite3
import XCTest

extension SQLiteAgentGroupChatStoreTests {
    func testMigration17RebuildsRoomTableWithCompositeManagerForeignKey() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        var initialStore: SQLiteAgentGroupChatStore? = try SQLiteAgentGroupChatStore(databaseURL: url)
        let manager = try await initialStore!.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "项目经理",
                rolePrompt: "管理任务。",
                modelConfigID: "model",
                professionKey: "project_manager"
            )
        )
        let room = try await initialStore!.createManagedRoom(
            ownerUserID: "alice",
            projectID: "migration-17-project",
            draft: .init(name: "迁移测试团队"),
            projectManagerAgentID: manager.id
        )
        initialStore = nil

        try executeSQLite(
            url,
            sql: """
            PRAGMA foreign_keys = OFF;
            BEGIN IMMEDIATE;
            CREATE TABLE project_agent_rooms_v16 (
                owner_user_id TEXT NOT NULL,
                id TEXT NOT NULL,
                project_id TEXT NOT NULL,
                name TEXT NOT NULL,
                goal TEXT NOT NULL,
                default_agent_id TEXT,
                status TEXT NOT NULL CHECK(status IN ('active', 'archived')),
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                conversation_kind TEXT NOT NULL DEFAULT 'project_team',
                direct_key TEXT,
                PRIMARY KEY(owner_user_id, id),
                FOREIGN KEY(owner_user_id, default_agent_id)
                    REFERENCES local_agent_profiles(owner_user_id, id)
            );
            INSERT INTO project_agent_rooms_v16
            SELECT owner_user_id, id, project_id, name, goal, default_agent_id,
                   status, created_at_unix_ms, updated_at_unix_ms,
                   conversation_kind, direct_key
            FROM project_agent_rooms;
            DROP TABLE project_agent_rooms;
            ALTER TABLE project_agent_rooms_v16 RENAME TO project_agent_rooms;
            CREATE UNIQUE INDEX one_active_agent_room_per_project
                ON project_agent_rooms(owner_user_id, project_id) WHERE status = 'active';
            CREATE UNIQUE INDEX one_active_direct_conversation_per_pair
                ON project_agent_rooms(owner_user_id, direct_key)
                WHERE status = 'active' AND direct_key IS NOT NULL;
            DELETE FROM local_agent_group_chat_schema_migrations WHERE version = 17;
            COMMIT;
            PRAGMA foreign_keys = ON;
            """
        )

        let migratedStore = try SQLiteAgentGroupChatStore(databaseURL: url)
        let migratedRoom = try await migratedStore.room(
            ownerUserID: "alice",
            roomID: room.id
        )
        XCTAssertEqual(migratedRoom?.projectManagerAgentID, manager.id)
        try executeSQLite(url, sql: "PRAGMA foreign_key_check;")
    }

    func testMigration22PreservesTodoDeliveryAndBackfillsEventRecipientOnReplay() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        var initialStore: SQLiteAgentGroupChatStore? = try SQLiteAgentGroupChatStore(databaseURL: url)
        let manager = try await initialStore!.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "项目经理",
                rolePrompt: "维护迁移任务板。",
                modelConfigID: "model",
                professionKey: "project_manager"
            )
        )
        let worker = try await makeAgent(initialStore!, name: "迁移执行者")
        let room = try await initialStore!.createManagedRoom(
            ownerUserID: "alice",
            projectID: "migration-22-project",
            draft: .init(name: "迁移事件团队"),
            projectManagerAgentID: manager.id
        )
        _ = try await initialStore!.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: worker.id,
            draft: .init(role: "执行者")
        )
        let todo = try await initialStore!.createAgentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            requestKey: "migration-22-todo",
            draft: .init(
                title: "验证事件迁移",
                teamRoomID: room.id,
                creatorAgentID: manager.id
            ),
            nowUnixMs: 100
        )
        let createdReadyDelivery = try await initialStore!.enqueueAgentTodoReady(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            nowUnixMs: 101
        )
        let readyDelivery = try XCTUnwrap(createdReadyDelivery)
        initialStore = nil

        try executeSQLite(
            url,
            sql: """
            PRAGMA foreign_keys = OFF;
            BEGIN IMMEDIATE;
            DROP TABLE local_agent_todo_event_recipients;
            DELETE FROM local_agent_group_chat_schema_migrations WHERE version = 22;
            COMMIT;
            PRAGMA foreign_keys = ON;
            """
        )

        let migratedStore = try SQLiteAgentGroupChatStore(databaseURL: url)
        let migratedTodo = try await migratedStore.agentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id
        )
        XCTAssertEqual(migratedTodo?.title, todo.title)
        let migratedDelivery = try await migratedStore.delivery(
            ownerUserID: "alice",
            deliveryID: readyDelivery.id
        )
        XCTAssertEqual(migratedDelivery?.id, readyDelivery.id)

        let replayed = try await migratedStore.enqueueAgentTodoReady(
            ownerUserID: "alice",
            agentID: worker.id,
            todoID: todo.id,
            nowUnixMs: 102
        )
        XCTAssertEqual(replayed?.id, readyDelivery.id)
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM local_agent_todo_event_recipients"
        ), 1)
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM local_agent_group_chat_schema_migrations WHERE version = 22"
        ), 1)
        try executeSQLite(url, sql: "PRAGMA foreign_key_check;")
    }

    func testMigration21RepairsMissingColumnEvenWhenMarkerAlreadyExists() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        var initialStore: SQLiteAgentGroupChatStore? = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(initialStore!, name: "迁移修复 Agent")
        let room = try await initialStore!.createRoom(
            ownerUserID: "alice",
            projectID: "migration-21-repair-project",
            draft: .init(name: "迁移修复团队")
        )
        _ = try await initialStore!.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "执行者")
        )
        let todo = try await initialStore!.createAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            requestKey: "migration-21-repair",
            draft: .init(title: "保留旧任务", teamRoomID: room.id),
            nowUnixMs: 100
        )
        initialStore = nil

        try executeSQLite(
            url,
            sql: """
            PRAGMA foreign_keys = OFF;
            ALTER TABLE local_agent_todos DROP COLUMN execution_contract_json;
            PRAGMA foreign_keys = ON;
            """
        )
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM local_agent_group_chat_schema_migrations WHERE version = 21"
        ), 1)
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM pragma_table_info('local_agent_todos') WHERE name = 'execution_contract_json'"
        ), 0)

        let repairedStore = try SQLiteAgentGroupChatStore(databaseURL: url)
        let repairedTodo = try await repairedStore.agentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            todoID: todo.id
        )
        XCTAssertEqual(repairedTodo?.title, todo.title)
        XCTAssertEqual(try sqliteInt(
            url,
            sql: "SELECT COUNT(*) FROM pragma_table_info('local_agent_todos') WHERE name = 'execution_contract_json'"
        ), 1)
    }

}
