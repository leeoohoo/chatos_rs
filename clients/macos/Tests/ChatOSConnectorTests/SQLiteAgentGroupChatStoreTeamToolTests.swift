@testable import ChatOSConnector
import ChatOSCore
import ChatOSAgentRuntime
import Foundation
import SQLite3
import XCTest

extension SQLiteAgentGroupChatStoreTests {
    func testTeamToolUsesOpaqueSingleChoiceAndProgramPassesThroughProjectID() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let localRoot = url.deletingLastPathComponent()
        let importedDirectory = localRoot.appendingPathComponent("import-target", isDirectory: true)
        try FileManager.default.createDirectory(
            at: importedDirectory,
            withIntermediateDirectories: true
        )
        let connectorStateURL = localRoot.appendingPathComponent("connector.json")
        var connectorState = NativeConnectorPersistentState.empty
        connectorState.user = .init(
            id: "alice",
            username: "alice",
            displayName: nil,
            role: "user"
        )
        connectorState.deviceID = "device"
        connectorState.workspaces = [.init(
            id: "workspace-1",
            alias: "workspace",
            absoluteRoot: localRoot.path,
            fingerprint: "fingerprint"
        )]
        try NativeConnectorStateStore(stateURL: connectorStateURL).save(connectorState)
        let connector = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "http://127.0.0.1:1")!,
                stateURL: connectorStateURL
            ),
            ticketProvider: NoNetworkTicketProvider()
        )
        let projectsService = NativeLocalProjectsService(
            connector: connector,
            databaseURL: localRoot.appendingPathComponent("projects.db")
        )
        let agent = try await makeAgent(
            store,
            name: "团队负责人",
            canAccessLocalProjects: true
        )
        XCTAssertEqual(agent.draft.professionKey, "general_member")
        let room = try await makeRoom(store, projectID: "source-project")
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "团队负责人")
        )
        let post = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "为设计项目创建团队",
                mentionedAgentIDs: [agent.id]
            ),
            limits: .init()
        )
        let claimed = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: post.message.createdAtUnixMs + 1
        )
        let delivery = try XCTUnwrap(claimed)
        let context = try LocalAgentChatRunContext(
            ownerUserID: "alice",
            projectID: room.projectID,
            roomID: room.id,
            agentID: agent.id,
            deliveryID: delivery.id,
            triggerMessageID: post.message.id,
            rootMessageID: post.message.rootMessageID,
            runID: "team-tool-run",
            hopCount: delivery.hopCount
        )
        let secretProjectID = "secret-project-id-never-given-to-model"
        let target = LocalProjectRecord(
            id: secretProjectID,
            ownerUserID: "alice",
            draft: .init(
                name: "设计系统",
                workspaceID: "workspace-1",
                relativeRoot: "design"
            ),
            createdAtUnixMs: 1,
            updatedAtUnixMs: 1
        )
        let productSkillSession = ProductToolSkillSession()
        let provider = LocalAgentProjectToolProvider(
            store: store,
            projects: [target],
            projectsService: projectsService,
            context: context,
            productSkillSession: productSkillSession,
            now: { post.message.createdAtUnixMs + 2 }
        )
        let executorContext = try LocalAgentChatRunContext(
            ownerUserID: context.ownerUserID,
            projectID: context.projectID,
            roomID: context.roomID,
            agentID: context.agentID,
            deliveryID: context.deliveryID,
            triggerMessageID: context.triggerMessageID,
            rootMessageID: context.rootMessageID,
            runID: "team-tool-executor-run",
            hopCount: context.hopCount,
            lane: .executor
        )
        let executorProvider = LocalAgentProjectToolProvider(
            store: store,
            projects: [target],
            projectsService: projectsService,
            context: executorContext,
            productSkillSession: productSkillSession,
            now: { post.message.createdAtUnixMs + 2 }
        )
        let executorDefinitions = try await executorProvider.definitions()
        XCTAssertTrue(executorDefinitions.isEmpty)
        do {
            _ = try await executorProvider.execute(.init(
                id: "executor-must-not-manage-projects",
                name: LocalAgentProjectToolProvider.catalogToolName,
                arguments: "{}"
            ))
            XCTFail("executor lane must not receive project/team management authority")
        } catch AgentGroupChatError.permissionDenied {
            // Expected: the provider also enforces the lane if it is mounted accidentally.
        }
        let definitions = try await provider.definitions()
        XCTAssertEqual(definitions.map(\.name), [
            "project_catalog",
            "team_propose_existing",
            "team_propose_new_project",
            "team_propose_import_directory",
        ])
        let coverage = ToolSkillCoverageCatalog.product.audit(definitions.map {
            .init(
                providerID: $0.providerID,
                toolName: $0.name,
                skillBindingID: $0.skillBindingID
            )
        })
        XCTAssertTrue(coverage.isComplete)
        XCTAssertEqual(coverage.coveredTools, 4)
        XCTAssertFalse(definitions.map(\.name).contains("team_propose"))
        for definition in definitions {
            XCTAssertTrue(
                definition.description.contains("不要求")
                    && definition.description.contains("项目经理"),
                definition.name
            )
        }
        let catalog = try await provider.execute(.init(
            id: "project-catalog-call",
            name: "project_catalog",
            arguments: "{}"
        ))
        XCTAssertTrue(catalog.content.contains(#""total_project_count":1"#))
        XCTAssertTrue(catalog.content.contains(#""available_for_team_count":1"#))
        XCTAssertTrue(catalog.content.contains("product-skill:chatos-project-team-setup"))
        let gatedProposal = try await provider.execute(.init(
            id: "proposal-before-skill-activation",
            name: "team_propose_new_project",
            arguments: #"{"project_name":"不应创建","project_type":"software_development","team_name":"未激活"}"#
        ))
        XCTAssertTrue(gatedProposal.isError)
        XCTAssertTrue(gatedProposal.content.contains("agent_skill_activate"))
        _ = try await productSkillSession.activate(
            skillRef: "product-skill:chatos-project-team-setup"
        )
        let teamDefinition = try XCTUnwrap(definitions.first {
            $0.name == LocalAgentProjectToolProvider.proposeExistingTeamToolName
        })
        let schema = String(decoding: teamDefinition.schema, as: UTF8.self)
        XCTAssertTrue(schema.contains("设计系统"))
        XCTAssertTrue(schema.contains("existing_1"))
        XCTAssertFalse(schema.contains(secretProjectID))
        XCTAssertFalse(schema.contains("absolute_path"))
        XCTAssertFalse(schema.contains("project_name"))
        let newProjectDefinition = try XCTUnwrap(definitions.first {
            $0.name == LocalAgentProjectToolProvider.proposeNewProjectTeamToolName
        })
        let newProjectSchema = String(
            decoding: newProjectDefinition.schema,
            as: UTF8.self
        )
        XCTAssertFalse(newProjectSchema.contains("absolute_path"))
        XCTAssertFalse(newProjectSchema.contains("project_option"))
        XCTAssertThrowsError(try AgentSchemaValidator.validate(
            arguments: #"{"project_name":"错误混参","project_type":"software_development","team_name":"团队","absolute_path":"/"}"#,
            schema: newProjectDefinition.schema
        ))
        let importDefinition = try XCTUnwrap(definitions.first {
            $0.name == LocalAgentProjectToolProvider.proposeImportedDirectoryTeamToolName
        })
        XCTAssertTrue(importDefinition.description.contains("GitHub/GitLab URL"))
        let importSchema = String(decoding: importDefinition.schema, as: UTF8.self)
        XCTAssertTrue(importSchema.contains("absolute_path"))
        XCTAssertFalse(importSchema.contains("project_option"))
        let rejectedRepositoryURL = try await provider.execute(.init(
            id: "remote-repository-must-not-be-imported",
            name: "team_propose_import_directory",
            arguments: #"{"absolute_path":"https://github.com/TencentCloud/CubeSandbox.git","project_type":"software_development","team_name":"错误团队"}"#
        ))
        XCTAssertTrue(rejectedRepositoryURL.isError)
        XCTAssertTrue(rejectedRepositoryURL.content.contains("不是本机路径"))
        XCTAssertTrue(rejectedRepositoryURL.content.contains("不得猜测或编造 /"))

        let result = try await provider.execute(.init(
            id: "team-proposal-call",
            name: "team_propose_existing",
            arguments: #"{"project_option":"existing_1","team_name":"设计团队","team_goal":"完成产品设计"}"#
        ))
        XCTAssertTrue(result.content.contains(#""status":"approved""#))
        XCTAssertFalse(result.content.contains(secretProjectID))
        let replayedResult = try await provider.execute(.init(
            id: "team-proposal-call",
            name: "team_propose_existing",
            arguments: #"{"project_option":"existing_1","team_name":"设计团队","team_goal":"完成产品设计"}"#
        ))
        XCTAssertEqual(replayedResult, result)
        let proposals = try await store.listTeamProposals(
            ownerUserID: "alice",
            sourceRoomID: room.id,
            status: .approved
        )
        let proposal = try XCTUnwrap(proposals.first(where: {
            $0.requestKey == "team-proposal-call"
        }))
        XCTAssertEqual(proposal.draft.existingProjectID, secretProjectID)
        XCTAssertEqual(proposals.count, 1)
        let pendingProposals = try await store.listTeamProposals(
            ownerUserID: "alice",
            sourceRoomID: room.id,
            status: .pending
        )
        XCTAssertTrue(pendingProposals.isEmpty)
        let activeRoom = try await store.activeRoom(
            ownerUserID: "alice",
            projectID: secretProjectID
        )
        let createdRoom = try XCTUnwrap(activeRoom)
        XCTAssertEqual(createdRoom.id, proposal.createdRoomID)
        XCTAssertEqual(createdRoom.draft.name, "设计团队")
        let sourceMessages = try await store.pageRecentMessages(
            ownerUserID: "alice",
            roomID: room.id,
            beforeMessageID: nil,
            limit: 50
        ).messages
        XCTAssertFalse(sourceMessages.contains { $0.causationID == proposal.id })
    }

    func testAgentCannotImpersonateHumanOrMentionNonMember() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let member = try await makeAgent(store, name: "成员")
        let outsider = try await makeAgent(store, name: "外部 Agent")
        let room = try await makeRoom(store)
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: member.id,
            draft: .init(role: "成员")
        )

        do {
            _ = try await store.postMessage(
                ownerUserID: "alice",
                roomID: room.id,
                draft: .init(
                    senderKind: .human,
                    senderID: member.id,
                    content: "伪造用户消息"
                ),
                limits: .init()
            )
            XCTFail("Impersonation was accepted")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .permissionDenied)
        }

        do {
            _ = try await store.postMessage(
                ownerUserID: "alice",
                roomID: room.id,
                draft: .init(
                    senderKind: .human,
                    senderID: "alice",
                    content: "@外部 Agent",
                    mentionedAgentIDs: [outsider.id]
                ),
                limits: .init()
            )
            XCTFail("Non-member mention was accepted")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .notMember)
        }
        let messages = try await store.listMessages(
            ownerUserID: "alice",
            roomID: room.id,
            afterUnixMs: nil,
            limit: 20
        )
        XCTAssertTrue(messages.isEmpty)
    }

    func testMentionMembershipValidationUsesOneQueryForTheWholeMessage() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let room = try await makeRoom(store)
        var mentionedAgentIDs: [String] = []
        for index in 0..<12 {
            let agent = try await makeAgent(store, name: "批量校验成员 \(index)")
            _ = try await store.addMember(
                ownerUserID: "alice",
                roomID: room.id,
                agentID: agent.id,
                draft: .init(role: "成员")
            )
            mentionedAgentIDs.append(agent.id)
        }
        let outsider = try await makeAgent(store, name: "批量校验外部成员")
        mentionedAgentIDs.append(outsider.id)

#if DEBUG
        let countBefore = await store.preparedStatementCountForTesting()
#endif
        do {
            _ = try await store.postMessage(
                ownerUserID: "alice",
                roomID: room.id,
                draft: .init(
                    senderKind: .human,
                    senderID: "alice",
                    content: "一次校验全部提及成员",
                    mentionedAgentIDs: mentionedAgentIDs
                ),
                limits: .init()
            )
            XCTFail("Non-member mention was accepted")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .notMember)
        }
#if DEBUG
        let queryCount = await store.preparedStatementCountForTesting() - countBefore
#endif
        let messages = try await store.listMessages(
            ownerUserID: "alice",
            roomID: room.id,
            afterUnixMs: nil,
            limit: 20
        )

        // BEGIN + room + active-members snapshot + ROLLBACK. The count is independent of the
        // number of mentioned Agents; the old path added one SELECT for every identifier.
#if DEBUG
        XCTAssertEqual(queryCount, 4)
#endif
        XCTAssertTrue(messages.isEmpty)

        let validMentionIDs = Array(mentionedAgentIDs.dropLast())
#if DEBUG
        let validCountBefore = await store.preparedStatementCountForTesting()
#endif
        let post = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "批量创建投递",
                mentionedAgentIDs: validMentionIDs
            ),
            limits: .init(maximumAgentRunsPerRootMessage: 32)
        )
#if DEBUG
        let validQueryCount = await store.preparedStatementCountForTesting() - validCountBefore
#endif

        // BEGIN + room + active-members snapshot + message + batched mentions + delivery count
        // + batched deliveries + COMMIT. The write count no longer grows with recipients.
#if DEBUG
        XCTAssertEqual(validQueryCount, 8)
#endif
        XCTAssertEqual(post.deliveries.count, validMentionIDs.count)
        XCTAssertEqual(post.deliveries.map(\.targetAgentID), validMentionIDs)
        XCTAssertTrue(post.deliveries.allSatisfy { $0.status == .pending && $0.attempt == 0 })
    }

    func testRoutingBudgetKeepsMessageButStopsAgentWakeup() async throws {
        let url = databaseURL()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = try SQLiteAgentGroupChatStore(databaseURL: url)
        let agent = try await makeAgent(store, name: "成员")
        let room = try await makeRoom(store)
        _ = try await store.addMember(
            ownerUserID: "alice",
            roomID: room.id,
            agentID: agent.id,
            draft: .init(role: "成员")
        )
        let posted = try await store.postMessage(
            ownerUserID: "alice",
            roomID: room.id,
            draft: .init(
                senderKind: .human,
                senderID: "alice",
                content: "超过深度仍应保存",
                mentionedAgentIDs: [agent.id],
                hopCount: 5
            ),
            limits: .init(maximumHopCount: 4, maximumAgentRunsPerRootMessage: 12)
        )
        XCTAssertTrue(posted.deliveries.isEmpty)
        XCTAssertNotNil(posted.routingStopReason)
        let messages = try await store.listMessages(
            ownerUserID: "alice",
            roomID: room.id,
            afterUnixMs: nil,
            limit: 20
        )
        XCTAssertEqual(messages.map(\.id), [posted.message.id])
    }

}
