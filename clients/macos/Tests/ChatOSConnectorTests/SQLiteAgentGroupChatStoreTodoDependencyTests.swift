@testable import ChatOSConnector
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import SQLite3
import XCTest

extension SQLiteAgentGroupChatStoreTests {
    func testTeamTodoDependenciesGateCrossAgentParallelExecutionAndRejectCycles() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let first = try await makeAgent(store, name: "前置负责人")
        let second = try await makeAgent(store, name: "后置负责人")
        let third = try await makeAgent(store, name: "等待负责人")
        let room = try await makeRoom(store, projectID: "dependency-project")
        for agent in [first, second, third] {
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: agent.draft.name)
            )
        }
        let prerequisite = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: first.id,
            requestKey: "dependency-prerequisite",
            draft: .init(title: "先完成接口", teamRoomID: room.id),
            nowUnixMs: 100
        )
        let dependent = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: second.id,
            requestKey: "dependency-dependent",
            draft: .init(
                title: "再实现客户端",
                teamRoomID: room.id,
                dependencies: [.init(
                    prerequisiteTodoID: prerequisite.id,
                    prerequisiteAgentID: first.id
                )]
            ),
            nowUnixMs: 101
        )

        let firstWave = try await store.enqueuePendingAgentTodos(
            ownerUserID: "alice",
            nowUnixMs: 102
        )
        XCTAssertEqual(firstWave.count, 1)
        XCTAssertTrue(firstWave[0].deduplicationKey.hasSuffix(prerequisite.id))
        XCTAssertFalse(firstWave[0].deduplicationKey.hasSuffix(dependent.id))

        let claimedValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: first.id,
            nowUnixMs: 103
        )
        let claimed = try XCTUnwrap(claimedValue)
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: first.id,
            todoID: prerequisite.id,
            update: .init(status: .completed, result: "接口已完成"),
            nowUnixMs: 104
        )
        _ = try await store.completeHeartbeatDelivery(
            ownerUserID: "alice",
            deliveryID: claimed.id,
            nowUnixMs: 105
        )
        let secondWave = try await store.enqueuePendingAgentTodos(
            ownerUserID: "alice",
            nowUnixMs: 106
        )
        XCTAssertEqual(secondWave.count, 1)
        XCTAssertTrue(secondWave[0].deduplicationKey.hasSuffix(dependent.id))

        let blockedPrerequisite = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: first.id,
            requestKey: "blocked-prerequisite",
            draft: .init(title: "阻塞的前置", teamRoomID: room.id),
            nowUnixMs: 107
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: first.id,
            todoID: blockedPrerequisite.id,
            update: .init(status: .inProgress),
            nowUnixMs: 108
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: first.id,
            todoID: blockedPrerequisite.id,
            update: .init(status: .blocked, blockedReason: "等待外部输入"),
            nowUnixMs: 109
        )
        let waitingOnBlocked = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: third.id,
            requestKey: "waiting-on-blocked",
            draft: .init(
                title: "不能越过阻塞前置",
                teamRoomID: room.id,
                dependencies: [.init(
                    prerequisiteTodoID: blockedPrerequisite.id,
                    prerequisiteAgentID: first.id
                )]
            ),
            nowUnixMs: 109
        )
        let cancelledPrerequisite = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: first.id,
            requestKey: "cancelled-prerequisite",
            draft: .init(title: "取消的前置", teamRoomID: room.id),
            nowUnixMs: 110
        )
        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: first.id,
            todoID: cancelledPrerequisite.id,
            update: .init(status: .cancelled),
            nowUnixMs: 111
        )
        let waitingOnCancelled = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: third.id,
            requestKey: "waiting-on-cancelled",
            draft: .init(
                title: "不能越过取消前置",
                teamRoomID: room.id,
                dependencies: [.init(
                    prerequisiteTodoID: cancelledPrerequisite.id,
                    prerequisiteAgentID: first.id
                )]
            ),
            nowUnixMs: 112
        )
        let blockedWave = try await store.enqueuePendingAgentTodos(
            ownerUserID: "alice",
            nowUnixMs: 113
        )
        XCTAssertFalse(blockedWave.contains {
            $0.deduplicationKey.hasSuffix(waitingOnBlocked.id)
                || $0.deduplicationKey.hasSuffix(waitingOnCancelled.id)
        })

        do {
            _ = try await store.setAgentTodoDependencies(
                ownerUserID: "alice",
                agentID: first.id,
                todoID: prerequisite.id,
                dependencies: [.init(
                    prerequisiteTodoID: dependent.id,
                    prerequisiteAgentID: second.id
                )],
                nowUnixMs: 114
            )
            XCTFail("A dependency cycle was accepted")
        } catch {
            XCTAssertEqual(
                error as? AgentGroupChatError,
                .invalidField("todoDependencyCycle")
            )
        }
    }

    func testManagerExplicitlyStartsOnlyHighestPriorityReadyTodoAndCancellationStopsExecutor() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let assignee = try await makeAgent(store, name: "执行者")
        let prerequisiteOwner = try await makeAgent(store, name: "前置负责人")
        let room = try await makeRoom(store, projectID: "explicit-scheduling-project")
        for agent in [assignee, prerequisiteOwner] {
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: agent.draft.name)
            )
        }
        let prerequisite = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: prerequisiteOwner.id,
            requestKey: "explicit-prerequisite",
            draft: .init(title: "尚未完成的前置", teamRoomID: room.id),
            nowUnixMs: 100
        )
        let blockedHighPriority = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: assignee.id,
            requestKey: "explicit-blocked",
            draft: .init(
                title: "有前置的高优先级任务",
                priority: 100,
                teamRoomID: room.id,
                dependencies: [.init(
                    prerequisiteTodoID: prerequisite.id,
                    prerequisiteAgentID: prerequisiteOwner.id
                )]
            ),
            nowUnixMs: 101
        )
        let ready = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: assignee.id,
            requestKey: "explicit-ready",
            draft: .init(title: "可立即执行", priority: 60, teamRoomID: room.id),
            nowUnixMs: 102
        )

        let startedDelivery = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: assignee.id,
            nowUnixMs: 103
        )
        let firstDelivery = try XCTUnwrap(startedDelivery)
        let todoLookupCountBefore = await store.preparedStatementCountForTesting()
        let selected = try await store.todoForDelivery(
            ownerUserID: "alice",
            deliveryID: firstDelivery.id
        )
        let todoLookupCount = await store.preparedStatementCountForTesting()
            - todoLookupCountBefore
        XCTAssertEqual(selected?.id, ready.id)
        XCTAssertEqual(todoLookupCount, 1)
        let stillBlocked = try await store.agentTodo(
            ownerUserID: "alice",
            agentID: assignee.id,
            todoID: blockedHighPriority.id
        )
        XCTAssertEqual(stillBlocked?.status, .pending)
        let secondDelivery = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: assignee.id,
            nowUnixMs: 104
        )
        XCTAssertNil(secondDelivery, "同一 Agent 已有 executor 时不能再启动第二个 Todo")

        _ = try await store.updateAgentTodo(
            ownerUserID: "alice",
            agentID: assignee.id,
            todoID: ready.id,
            update: .init(status: .cancelled),
            nowUnixMs: 105
        )
        let cancelledDelivery = try await store.delivery(
            ownerUserID: "alice",
            deliveryID: firstDelivery.id
        )
        XCTAssertEqual(cancelledDelivery?.status, .cancelled)
        do {
            _ = try await store.updateAgentTodo(
                ownerUserID: "alice",
                agentID: assignee.id,
                todoID: ready.id,
                update: .init(status: .completed, result: "不应写入"),
                nowUnixMs: 106
            )
            XCTFail("Cancelled executor completed its Todo")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .conflict)
        }
        let noReadyDelivery = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: assignee.id,
            nowUnixMs: 107
        )
        XCTAssertNil(noReadyDelivery, "前置未完成的 Todo 不能被启动")
    }

    func testTeamAssetsRequireProjectManagerAndTodoKeepsStartRevisionSnapshot() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let manager = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "项目经理",
                rolePrompt: "维护团队资产与任务板。",
                modelConfigID: "model-1",
                professionKey: "project_manager"
            )
        )
        let worker = try await makeAgent(store, name: "工程师")
        let room = try await store.createManagedRoom(
            ownerUserID: "alice",
            projectID: "asset-snapshot-project",
            draft: .init(name: "资产快照团队"),
            projectManagerAgentID: manager.id
        )
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: worker.id,
            draft: .init(role: "工程师")
        )

        do {
            _ = try await store.upsertTeamAsset(
                ownerUserID: "alice",
                teamRoomID: room.id,
                assetID: nil,
                editorAgentID: worker.id,
                category: .overview,
                title: "项目背景",
                markdown: "普通成员不应写入",
                expectedRevision: nil,
                nowUnixMs: 200
            )
            XCTFail("A non-manager wrote a team asset")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .permissionDenied)
        }

        let firstRevision = try await store.upsertTeamAsset(
            ownerUserID: "alice",
            teamRoomID: room.id,
            assetID: nil,
            editorAgentID: manager.id,
            category: .techStack,
            title: "技术栈",
            markdown: "Swift 6 + SQLite",
            expectedRevision: nil,
            nowUnixMs: 201
        )
        do {
            _ = try await store.upsertTeamAsset(
                ownerUserID: "alice",
                teamRoomID: room.id,
                assetID: firstRevision.id,
                editorAgentID: manager.id,
                category: .techStack,
                title: "技术栈",
                markdown: "错误覆盖",
                expectedRevision: 0,
                nowUnixMs: 202
            )
            XCTFail("A stale asset revision was accepted")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .conflict)
        }

        let todo = try await store.createAgentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            requestKey: "asset-snapshot-todo",
            draft: .init(
                title: "实现客户端",
                teamRoomID: room.id,
                creatorAgentID: manager.id
            ),
            nowUnixMs: 203
        )
        let executorDelivery = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: worker.id,
            nowUnixMs: 204
        )
        _ = try XCTUnwrap(executorDelivery)
        let updatedAsset = try await store.upsertTeamAsset(
            ownerUserID: "alice",
            teamRoomID: room.id,
            assetID: firstRevision.id,
            editorAgentID: manager.id,
            category: .techStack,
            title: "技术栈",
            markdown: "Swift 6 + PostgreSQL",
            expectedRevision: firstRevision.revision,
            nowUnixMs: 205
        )
        XCTAssertEqual(updatedAsset.revision, 2)

        let snapshots = try await store.listTodoTeamAssetSnapshots(
            ownerUserID: "alice",
            todoID: todo.id
        )
        XCTAssertEqual(snapshots.count, 1)
        XCTAssertEqual(snapshots[0].assetID, firstRevision.id)
        XCTAssertEqual(snapshots[0].revision, 1)
        XCTAssertEqual(snapshots[0].markdown, "Swift 6 + SQLite")
        let revisionOneSnapshot = try await store.todoTeamAssetSnapshot(
            ownerUserID: "alice",
            todoID: todo.id,
            assetID: firstRevision.id,
            revision: 1
        )
        XCTAssertNotNil(revisionOneSnapshot)
        let revisionTwoSnapshot = try await store.todoTeamAssetSnapshot(
            ownerUserID: "alice",
            todoID: todo.id,
            assetID: firstRevision.id,
            revision: 2
        )
        XCTAssertNil(revisionTwoSnapshot)
        let revisions = try await store.listTeamAssetRevisions(
            ownerUserID: "alice",
            teamRoomID: room.id,
            assetID: firstRevision.id,
            limit: 10
        )
        XCTAssertEqual(revisions.map(\.revision), [2, 1])
    }

    func testManagedTeamRequiresExplicitProjectManagerProfession() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let ordinary = try await makeAgent(store, name: "普通成员")
        do {
            _ = try await store.createManagedRoom(
                ownerUserID: "alice",
                projectID: "invalid-managed-team",
                draft: .init(name: "无项目经理团队"),
                projectManagerAgentID: ordinary.id
            )
            XCTFail("A non-project-manager profession created a managed team")
        } catch {
            XCTAssertEqual(
                error as? AgentGroupChatError,
                .invalidField("projectManagerProfession")
            )
        }
        let manager = try await store.createAgent(
            ownerUserID: "alice",
            draft: .init(
                name: "项目经理",
                rolePrompt: "维护任务板。",
                modelConfigID: "model",
                professionKey: "project_manager"
            )
        )
        let room = try await store.createManagedRoom(
            ownerUserID: "alice",
            projectID: "managed-team",
            draft: .init(name: "有项目经理团队"),
            projectManagerAgentID: manager.id
        )
        XCTAssertEqual(room.projectManagerAgentID, manager.id)
        XCTAssertEqual(room.defaultAgentID, manager.id)
        let members = try await store.listMembers(ownerUserID: "alice", roomID: room.id)
        XCTAssertEqual(members.map(\.agentID), [manager.id])
    }

}
