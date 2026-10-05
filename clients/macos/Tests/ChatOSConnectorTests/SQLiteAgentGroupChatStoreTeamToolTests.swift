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

        let newProjectResult = try await provider.execute(.init(
            id: "new-project-team-proposal-call",
            name: "team_propose_new_project",
            arguments: #"{"project_name":"新建调研项目","project_description":"独立新项目","project_type":"software_development","team_name":"新项目团队","team_goal":"完成调研"}"#
        ))
        XCTAssertFalse(newProjectResult.isError)
        XCTAssertTrue(newProjectResult.content.contains(#""creates_new_project":true"#))

        let importResult = try await provider.execute(.init(
            id: "import-directory-team-proposal-call",
            name: "team_propose_import_directory",
            arguments: try toolArguments([
                "absolute_path": importedDirectory.path,
                "project_type": "software_development",
                "team_name": "现有目录团队",
                "team_goal": "在原目录工作",
            ])
        ))
        XCTAssertFalse(importResult.isError)
        XCTAssertTrue(importResult.content.contains(#""creates_new_project":true"#))

        let result = try await provider.execute(.init(
            id: "team-proposal-call",
            name: "team_propose_existing",
            arguments: #"{"project_option":"existing_1","team_name":"设计团队","team_goal":"完成产品设计"}"#
        ))
        XCTAssertFalse(result.content.contains(secretProjectID))
        let proposals = try await store.listTeamProposals(
            ownerUserID: "alice",
            sourceRoomID: room.id,
            status: .pending
        )
        let proposal = try XCTUnwrap(proposals.first(where: {
            $0.requestKey == "team-proposal-call"
        }))
        XCTAssertEqual(proposal.draft.existingProjectID, secretProjectID)
        let newProjectProposal = try XCTUnwrap(proposals.first(where: {
            $0.requestKey == "new-project-team-proposal-call"
        }))
        XCTAssertEqual(newProjectProposal.draft.newProjectName, "新建调研项目")
        XCTAssertNil(newProjectProposal.draft.importedProjectDraft)
        let importedProposal = try XCTUnwrap(proposals.first(where: {
            $0.requestKey == "import-directory-team-proposal-call"
        }))
        XCTAssertEqual(importedProposal.draft.importedProjectAbsolutePath, importedDirectory.path)
        XCTAssertEqual(importedProposal.draft.importedProjectDraft?.relativeRoot, "import-target")
        XCTAssertNil(importedProposal.draft.newProjectName)

        // The proposal was submitted from a running communication cycle. Resolve that source
        // delivery before the Human decision so the follow-up delivery can be claimed normally.
        _ = try await store.failDelivery(
            ownerUserID: "alice",
            deliveryID: delivery.id,
            error: "test source cycle finished",
            nowUnixMs: post.message.createdAtUnixMs + 3
        )

        do {
            _ = try await store.approveTeamProposal(
                ownerUserID: "alice",
                sourceRoomID: room.id,
                proposalID: proposal.id,
                resolvedProjectID: "wrong-project-id",
                nowUnixMs: post.message.createdAtUnixMs + 3
            )
            XCTFail("Human approval changed the program-resolved project id")
        } catch {
            XCTAssertEqual(error as? AgentGroupChatError, .permissionDenied)
        }
        let approval = try await store.approveTeamProposal(
            ownerUserID: "alice",
            sourceRoomID: room.id,
            proposalID: proposal.id,
            resolvedProjectID: secretProjectID,
            nowUnixMs: post.message.createdAtUnixMs + 3
        )
        XCTAssertEqual(approval.room.projectID, secretProjectID)
        let claimedResolutionDelivery = try await store.claimNextDelivery(
            ownerUserID: "alice",
            agentID: agent.id,
            nowUnixMs: post.message.createdAtUnixMs + 4
        )
        let resolutionDelivery = try XCTUnwrap(claimedResolutionDelivery)
        XCTAssertEqual(resolutionDelivery.roomID, room.id)
        XCTAssertEqual(resolutionDelivery.targetAgentID, agent.id)
        XCTAssertEqual(resolutionDelivery.triggerKind, .mention)
        let loadedResolutionMessage = try await store.message(
            ownerUserID: "alice",
            roomID: room.id,
            messageID: resolutionDelivery.messageID
        )
        let resolutionMessage = try XCTUnwrap(loadedResolutionMessage)
        XCTAssertEqual(resolutionMessage.senderKind, .system)
        XCTAssertEqual(resolutionMessage.causationID, proposal.id)
        XCTAssertEqual(resolutionMessage.mentionedAgentIDs, [agent.id])
        XCTAssertTrue(resolutionMessage.content.contains("Human 已批准"))
        XCTAssertTrue(resolutionMessage.content.contains("不要等待 Human 再次提醒"))
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

        let countBefore = await store.preparedStatementCountForTesting()
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
        let queryCount = await store.preparedStatementCountForTesting() - countBefore
        let messages = try await store.listMessages(
            ownerUserID: "alice",
            roomID: room.id,
            afterUnixMs: nil,
            limit: 20
        )

        // BEGIN + room + active-members snapshot + ROLLBACK. The count is independent of the
        // number of mentioned Agents; the old path added one SELECT for every identifier.
        XCTAssertEqual(queryCount, 4)
        XCTAssertTrue(messages.isEmpty)

        let validMentionIDs = Array(mentionedAgentIDs.dropLast())
        let validCountBefore = await store.preparedStatementCountForTesting()
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
        let validQueryCount = await store.preparedStatementCountForTesting() - validCountBefore

        // BEGIN + room + active-members snapshot + message + batched mentions + delivery count
        // + batched deliveries + COMMIT. The write count no longer grows with recipients.
        XCTAssertEqual(validQueryCount, 8)
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
