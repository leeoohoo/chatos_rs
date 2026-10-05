@testable import ChatOSConnector
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import SQLite3
import XCTest

extension SQLiteAgentGroupChatStoreTests {
    func testTodoRunSummaryProjectsCommittedPathsWithoutReturningManagerRuns() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "摘要执行者")
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
            requestKey: "summary-todo",
            draft: .init(title: "生成轻量摘要", teamRoomID: room.id),
            nowUnixMs: 100
        )
        let deliveryValue = try await store.startNextReadyAgentTodo(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: 101
        )
        let delivery = try XCTUnwrap(deliveryValue)
        let runID = UUID()
        let context = try LocalAgentChatRunContext(
            ownerUserID: "alice",
            projectID: room.projectID,
            roomID: room.id,
            agentID: agent.id,
            deliveryID: delivery.id,
            triggerMessageID: delivery.messageID,
            rootMessageID: delivery.rootMessageID,
            runID: runID.uuidString.lowercased(),
            hopCount: delivery.hopCount,
            lane: .executor
        )
        var checkpoint = AgentRunCheckpoint(
            scope: LocalAgentGroupChatRun.runtimeScope(for: context),
            messages: [.init(role: .system, content: "large history is not needed here")]
        )
        checkpoint.id = runID
        checkpoint.status = .completed
        checkpoint.receipts = [
            "write": .init(#"{"result":{"committed_paths":["Sources/B.swift","Sources/A.swift"]}}"#),
            "duplicate": .init(#"{"committed_paths":["Sources/A.swift"]}"#),
            "failed": .failure(#"{"committed_paths":["Sources/Failed.swift"]}"#),
        ]
        try await store.saveRun(try LocalAgentGroupChatRun(
            id: runID,
            context: context,
            modelConfigID: agent.draft.modelConfigID,
            policy: .init(),
            checkpoint: checkpoint,
            createdAtUnixMs: 101,
            updatedAtUnixMs: 102
        ))

        let summaries = try await store.listLatestTodoRunSummaries(
            ownerUserID: "alice",
            roomID: room.id,
            limit: 500
        )

        XCTAssertEqual(summaries, [
            .init(
                todoID: todo.id,
                runID: runID,
                status: .completed,
                receiptCount: 3,
                committedPaths: ["Sources/A.swift", "Sources/B.swift"],
                updatedAtUnixMs: 102
            ),
        ])
    }

    func testStopOutstandingDeliveriesClosesRunsAndCancelsQueuedWorkAtomically() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let runningAgent = try await makeAgent(store, name: "运行成员")
        let queuedAgent = try await makeAgent(store, name: "排队成员")
        let room = try await makeRoom(store)
        for agent in [runningAgent, queuedAgent] {
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: agent.draft.name)
            )
        }
        let post = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "同时停止",
                mentionedAgentIDs: [runningAgent.id, queuedAgent.id]
            ),
            limits: .init()
        )
        let runningValue = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: runningAgent.id,
            nowUnixMs: post.message.createdAtUnixMs + 1
        )
        let running = try XCTUnwrap(runningValue)
        let queued = try XCTUnwrap(post.deliveries.first(where: { $0.targetAgentID == queuedAgent.id }))
        let runID = UUID()
        let context = try LocalAgentChatRunContext(
            ownerUserID: "alice",
            projectID: "project-1",
            roomID: room.id,
            agentID: runningAgent.id,
            deliveryID: running.id,
            triggerMessageID: post.message.id,
            rootMessageID: post.message.rootMessageID,
            runID: runID.uuidString.lowercased(),
            hopCount: running.hopCount
        )
        var checkpoint = AgentRunCheckpoint(
            scope: LocalAgentGroupChatRun.runtimeScope(for: context),
            messages: [.init(role: .system, content: "system")]
        )
        checkpoint.id = runID
        checkpoint.status = .paused
        let run = try LocalAgentGroupChatRun(
            id: runID,
            context: context,
            modelConfigID: runningAgent.draft.modelConfigID,
            policy: .init(),
            checkpoint: checkpoint,
            createdAtUnixMs: post.message.createdAtUnixMs + 1,
            updatedAtUnixMs: post.message.createdAtUnixMs + 1
        )
        try await store.saveRun(run)

        let stopped = try await store.stopOutstandingDeliveries(
            ownerUserID: "alice",
            roomID: room.id,
            reason: "用户停止全部 Agent。",
            nowUnixMs: post.message.createdAtUnixMs + 2
        )
        XCTAssertEqual(stopped, 2)
        let stoppedRunning = try await store.delivery(ownerUserID: "alice", deliveryID: running.id)
        let stoppedQueued = try await store.delivery(ownerUserID: "alice", deliveryID: queued.id)
        XCTAssertEqual(stoppedRunning?.status, .failed)
        XCTAssertEqual(stoppedQueued?.status, .cancelled)
        let closedRun = try await store.run(ownerUserID: "alice", deliveryID: running.id)
        XCTAssertEqual(closedRun?.checkpoint.status, .failed)
        XCTAssertEqual(closedRun?.checkpoint.stopReason, "用户停止全部 Agent。")
        XCTAssertEqual(closedRun?.events.last?.kind, "stopped_all")
        let unfinished = try await store.listUnfinishedRuns(
            ownerUserID: "alice",
            projectID: "project-1",
            limit: 10
        )
        XCTAssertTrue(unfinished.isEmpty)
    }

    func testStopOutstandingDeliveriesLoadsAllRunsWithOneQuery() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let room = try await makeRoom(store, projectID: "stop-batch-project")
        var agents: [LocalAgentProfile] = []
        for index in 0..<12 {
            let agent = try await makeAgent(store, name: "停止批量 Agent \(index)")
            agents.append(agent)
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: "执行者")
            )
        }
        let post = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "批量停止",
                mentionedAgentIDs: agents.map(\.id)
            ),
            limits: .init(maximumAgentRunsPerRootMessage: 20)
        )
        for agent in agents {
            let claimed = try await store.claimNextDelivery(
                ownerUserID: "alice",
                agentID: agent.id,
                nowUnixMs: post.message.createdAtUnixMs + 1
            )
            let delivery = try XCTUnwrap(claimed)
            let runID = UUID()
            let context = try LocalAgentChatRunContext(
                ownerUserID: "alice",
                projectID: "stop-batch-project",
                roomID: room.id,
                agentID: agent.id,
                deliveryID: delivery.id,
                triggerMessageID: post.message.id,
                rootMessageID: post.message.rootMessageID,
                runID: runID.uuidString.lowercased(),
                hopCount: delivery.hopCount
            )
            var checkpoint = AgentRunCheckpoint(
                scope: LocalAgentGroupChatRun.runtimeScope(for: context),
                messages: [.init(role: .system, content: "system")]
            )
            checkpoint.id = runID
            checkpoint.status = .paused
            try await store.saveRun(LocalAgentGroupChatRun(
                id: runID,
                context: context,
                modelConfigID: agent.draft.modelConfigID,
                policy: .init(),
                checkpoint: checkpoint,
                createdAtUnixMs: post.message.createdAtUnixMs + 1,
                updatedAtUnixMs: post.message.createdAtUnixMs + 1
            ))
        }

        let before = await store.preparedStatementCountForTesting()
        let stopped = try await store.stopOutstandingDeliveries(
            ownerUserID: "alice",
            roomID: room.id,
            reason: "用户批量停止全部 Agent。",
            nowUnixMs: post.message.createdAtUnixMs + 2
        )
        let statementCount = await store.preparedStatementCountForTesting() - before

        XCTAssertEqual(stopped, 12)
        XCTAssertEqual(statementCount, 7)
        let runs = try await store.listRoomRuns(
            ownerUserID: "alice",
            roomID: room.id,
            limit: 20
        )
        XCTAssertEqual(runs.count, 12)
        XCTAssertTrue(runs.allSatisfy { $0.checkpoint.status == .failed })
        XCTAssertTrue(runs.allSatisfy { $0.checkpoint.stopReason == "用户批量停止全部 Agent。" })
    }

    func testAgentProfileAndProjectMembershipUpdateTogether() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "旧名称")
        let room = try await makeRoom(store)
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "旧角色", pluginAllowlist: ["plugin.old"])
        )

        let result = try await store.updateAgentMembership(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            profileDraft: .init(
                name: "新名称",
                description: "新职责",
                rolePrompt: "使用新的角色指令。",
                modelConfigID: "model-2",
                thinkingLevel: "high",
                defaultPluginIDs: ["plugin.new"],
                defaultSkillIDs: ["skill.keep"]
            ),
            memberDraft: .init(
                role: "新角色",
                responsibility: "新职责",
                pluginAllowlist: ["plugin.new"]
            )
        )

        XCTAssertEqual(result.profile.draft.name, "新名称")
        XCTAssertEqual(result.profile.draft.modelConfigID, "model-2")
        XCTAssertEqual(result.profile.draft.thinkingLevel, "high")
        XCTAssertEqual(result.profile.draft.defaultSkillIDs, ["skill.keep"])
        XCTAssertEqual(result.member.draft.role, "新角色")
        XCTAssertEqual(result.member.draft.pluginAllowlist, ["plugin.new"])
        let profiles = try await store.listAgents(ownerUserID: "alice", includeArchived: false)
        let members = try await store.listMembers(ownerUserID: "alice", roomID: room.id)
        XCTAssertEqual(profiles.first(where: { $0.id == agent.id }), result.profile)
        XCTAssertEqual(members.first(where: { $0.agentID == agent.id }), result.member)
    }

}
