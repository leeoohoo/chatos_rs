import ChatOSCore
import Foundation
import XCTest

final class AgentGroupChatCodableContractTests: XCTestCase {
    private func encodedObject<T: Encodable>(_ value: T) throws -> [String: Any] {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        return try XCTUnwrap(
            JSONSerialization.jsonObject(with: encoder.encode(value)) as? [String: Any]
        )
    }

    private func assertRoundTrip<T: Codable & Equatable>(
        _ value: T,
        expectedKeys: Set<String>,
        file: StaticString = #filePath,
        line: UInt = #line
    ) throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        let data = try encoder.encode(value)
        let object = try XCTUnwrap(
            JSONSerialization.jsonObject(with: data) as? [String: Any],
            file: file,
            line: line
        )
        XCTAssertEqual(Set(object.keys), expectedKeys, file: file, line: line)
        XCTAssertEqual(try JSONDecoder().decode(T.self, from: data), value, file: file, line: line)
    }

    func testErrorDescriptionsAndPersistedEnumRawValuesRemainStable() {
        XCTAssertEqual(
            AgentGroupChatError.invalidField("field").localizedDescription,
            "无效的 Agent 群聊字段：field"
        )
        XCTAssertEqual(AgentGroupChatError.notFound.localizedDescription, "Agent 群聊资源不存在。")
        XCTAssertEqual(
            AgentGroupChatError.conflict.localizedDescription,
            "Agent 群聊状态已经变化，请刷新后重试。"
        )
        XCTAssertEqual(AgentGroupChatError.notMember.localizedDescription, "Agent 不是当前群聊成员。")
        XCTAssertEqual(
            AgentGroupChatError.permissionDenied.localizedDescription,
            "当前身份不能执行这个群聊操作。"
        )
        XCTAssertEqual(
            AgentGroupChatError.storage("disk").localizedDescription,
            "本地 Agent 群聊存储不可用：disk"
        )

        XCTAssertEqual(LocalAgentConversationKind.projectTeam.rawValue, "project_team")
        XCTAssertEqual(LocalAgentConversationKind.humanAgentDirect.rawValue, "human_agent_direct")
        XCTAssertEqual(LocalAgentConversationKind.agentAgentDirect.rawValue, "agent_agent_direct")
        XCTAssertEqual(LocalAgentTodoStatus.inProgress.rawValue, "in_progress")
        XCTAssertEqual(LocalAgentTeamAssetCategory.currentProgress.rawValue, "current_progress")
        XCTAssertEqual(LocalAgentTeamAssetCategory.techStack.rawValue, "tech_stack")
        XCTAssertEqual(ProjectAgentDeliveryTriggerKind.defaultAgent.rawValue, "default_agent")
        XCTAssertEqual(ProjectAgentDeliveryTriggerKind.agentMention.rawValue, "agent_mention")
        XCTAssertEqual(ProjectAgentDeliveryTriggerKind.todoStatus.rawValue, "todo_status")
        XCTAssertEqual(ProjectAgentMessageAttachmentSyncStatus.localOnly.rawValue, "local_only")
    }

    func testProfileAndConversationCodableKeysRemainStable() throws {
        let profile = LocalAgentProfile(
            id: "agent-1",
            ownerUserID: "owner-1",
            draft: .init(
                name: "Architect",
                description: "Owns architecture",
                rolePrompt: "Preserve behavior.",
                modelConfigID: "model-1",
                thinkingLevel: "high",
                professionKey: LocalAgentSkillCatalog.legacyProfessionKey,
                defaultPluginIDs: ["plugin-1"],
                defaultSkillIDs: ["skill-1"],
                heartbeatEnabled: true,
                heartbeatIntervalSeconds: 600,
                heartbeatPrompt: "Check work"
            ),
            status: .active,
            createdAtUnixMs: 100,
            updatedAtUnixMs: 200,
            lastHeartbeatAtUnixMs: 150,
            nextHeartbeatAtUnixMs: 750
        )
        try assertRoundTrip(profile, expectedKeys: [
            "id", "ownerUserID", "draft", "status", "createdAtUnixMs", "updatedAtUnixMs",
            "lastHeartbeatAtUnixMs", "nextHeartbeatAtUnixMs",
        ])
        let profileObject = try encodedObject(profile)
        let draftObject = try XCTUnwrap(profileObject["draft"] as? [String: Any])
        XCTAssertEqual(Set(draftObject.keys), [
            "name", "description", "rolePrompt", "modelConfigID", "thinkingLevel",
            "professionKey", "defaultPluginIDs", "defaultSkillIDs", "heartbeatEnabled",
            "heartbeatIntervalSeconds", "heartbeatPrompt",
        ])

        let team = ProjectAgentRoom(
            id: "room-1",
            ownerUserID: "owner-1",
            projectID: "project-1",
            draft: .init(name: "Team", goal: "Ship safely"),
            defaultAgentID: "agent-1",
            projectManagerAgentID: "agent-2",
            createdAtUnixMs: 100,
            updatedAtUnixMs: 200
        )
        try assertRoundTrip(team, expectedKeys: [
            "id", "ownerUserID", "projectID", "draft", "defaultAgentID",
            "projectManagerAgentID", "conversationKind", "status", "createdAtUnixMs",
            "updatedAtUnixMs",
        ])

        let direct = ProjectAgentRoom(
            id: "room-2",
            ownerUserID: "owner-1",
            projectID: "direct-room-2",
            draft: .init(name: "Direct"),
            defaultAgentID: "agent-1",
            conversationKind: .humanAgentDirect,
            directKey: "owner-1-agent-1",
            createdAtUnixMs: 100,
            updatedAtUnixMs: 100
        )
        try assertRoundTrip(direct, expectedKeys: [
            "id", "ownerUserID", "projectID", "draft", "defaultAgentID", "conversationKind",
            "directKey", "status", "createdAtUnixMs", "updatedAtUnixMs",
        ])
    }

    func testMessageAndAttachmentCodableKeysRemainStable() throws {
        let attachment = ProjectAgentMessageAttachment(
            id: "attachment-1",
            name: "report.md",
            mimeType: "text/markdown",
            size: 42,
            kind: .file,
            origin: .pastedDocument,
            sha256: String(repeating: "a", count: 64),
            syncStatus: .synced,
            artifactID: "artifact-1",
            storageProvider: "minio",
            bucket: "private",
            objectKey: "objects/1",
            remoteViewPath: "/api/agent-artifacts/artifact-1/content",
            uploadError: "previous retry",
            syncedAtUnixMs: 300
        )
        try assertRoundTrip(attachment, expectedKeys: [
            "id", "name", "mimeType", "size", "kind", "origin", "sha256", "syncStatus",
            "artifactID", "storageProvider", "bucket", "objectKey", "remoteViewPath",
            "uploadError", "syncedAtUnixMs",
        ])

        let message = ProjectAgentMessage(
            id: "message-1",
            ownerUserID: "owner-1",
            roomID: "room-1",
            draft: .init(
                senderKind: .agent,
                senderID: "agent-1",
                content: "Summary",
                mentionedAgentIDs: ["agent-2"],
                replyToMessageID: "message-0",
                sourceRunID: "run-1",
                causationID: "cause-1",
                rootMessageID: "message-0",
                hopCount: 2
            ),
            rootMessageID: "message-0",
            attachments: [attachment],
            createdAtUnixMs: 400
        )
        try assertRoundTrip(message, expectedKeys: [
            "id", "ownerUserID", "roomID", "senderKind", "senderID", "content",
            "mentionedAgentIDs", "replyToMessageID", "sourceRunID", "causationID",
            "rootMessageID", "hopCount", "createdAtUnixMs", "attachments",
        ])
    }

    func testTodoAssetAndDeliveryCodableKeysRemainStable() throws {
        let todo = LocalAgentTodo(
            id: "todo-1",
            ownerUserID: "owner-1",
            agentID: "agent-1",
            teamRoomID: "room-1",
            sourceRoomID: "room-1",
            sourceMessageID: "message-1",
            title: "Implement contract",
            detail: "Keep the wire format stable.",
            priority: 80,
            sortOrder: 5,
            status: .inProgress,
            blockedReason: "",
            result: "",
            executionContract: .init(
                objective: "Implement contract",
                scope: "Core only",
                expectedOutputs: ["Tests"],
                acceptanceCriteria: ["Round-trip passes"],
                constraints: ["No behavior changes"]
            ),
            executionPlan: .init(
                requiresExecution: true,
                builtinCapabilities: [.projectRead, .projectWrite],
                plugins: [.init(pluginID: "plugin-1", displayName: "Plugin", reason: "Verify")],
                selectionRevision: "revision-1",
                selectedAtUnixMs: 90
            ),
            createdAtUnixMs: 100,
            updatedAtUnixMs: 200
        )
        try assertRoundTrip(todo, expectedKeys: [
            "id", "ownerUserID", "agentID", "teamRoomID", "sourceRoomID", "sourceMessageID",
            "title", "detail", "priority", "sortOrder", "status", "blockedReason", "result",
            "executionContract", "executionPlan", "createdAtUnixMs", "updatedAtUnixMs",
        ])
        let todoObject = try encodedObject(todo)
        XCTAssertEqual(
            Set(try XCTUnwrap(todoObject["executionContract"] as? [String: Any]).keys),
            ["objective", "scope", "expectedOutputs", "acceptanceCriteria", "constraints"]
        )
        XCTAssertEqual(
            Set(try XCTUnwrap(todoObject["executionPlan"] as? [String: Any]).keys),
            ["requiresExecution", "builtinCapabilities", "plugins", "selectionRevision", "selectedAtUnixMs"]
        )

        let asset = LocalAgentTeamAsset(
            id: "asset-1",
            ownerUserID: "owner-1",
            teamRoomID: "room-1",
            category: .architecture,
            title: "Architecture",
            markdown: "# Architecture",
            revision: 2,
            status: .active,
            createdByAgentID: "agent-1",
            updatedByAgentID: "agent-2",
            createdAtUnixMs: 100,
            updatedAtUnixMs: 200
        )
        try assertRoundTrip(asset, expectedKeys: [
            "id", "ownerUserID", "teamRoomID", "category", "title", "markdown", "revision",
            "status", "createdByAgentID", "updatedByAgentID", "createdAtUnixMs", "updatedAtUnixMs",
        ])

        let delivery = ProjectAgentDelivery(
            id: "delivery-1",
            ownerUserID: "owner-1",
            roomID: "room-1",
            messageID: "message-1",
            rootMessageID: "message-0",
            targetAgentID: "agent-1",
            triggerKind: .todoStatus,
            status: .completed,
            attempt: 2,
            hopCount: 1,
            deduplicationKey: "dedupe-1",
            responseMessageID: "message-2",
            lastError: "previous retry",
            claimedAtUnixMs: 110,
            completedAtUnixMs: 130,
            createdAtUnixMs: 100
        )
        try assertRoundTrip(delivery, expectedKeys: [
            "id", "ownerUserID", "roomID", "messageID", "rootMessageID", "targetAgentID",
            "triggerKind", "status", "attempt", "hopCount", "deduplicationKey",
            "responseMessageID", "lastError", "claimedAtUnixMs", "completedAtUnixMs",
            "createdAtUnixMs",
        ])
        XCTAssertEqual(delivery.lane, .manager)
    }

    func testLegacyDefaultsRemainDecodable() throws {
        let legacyDraft = try JSONDecoder().decode(
            LocalAgentProfileDraft.self,
            from: Data(#"{"name":"Legacy","rolePrompt":"Keep working","modelConfigID":"model-1"}"#.utf8)
        )
        XCTAssertEqual(legacyDraft.description, "")
        XCTAssertNil(legacyDraft.avatarData)
        XCTAssertNil(legacyDraft.thinkingLevel)
        XCTAssertEqual(legacyDraft.professionKey, LocalAgentSkillCatalog.legacyProfessionKey)
        XCTAssertEqual(legacyDraft.defaultPluginIDs, [])
        XCTAssertEqual(legacyDraft.defaultSkillIDs, [])
        XCTAssertFalse(legacyDraft.heartbeatEnabled)
        XCTAssertEqual(legacyDraft.heartbeatIntervalSeconds, 900)
        XCTAssertEqual(legacyDraft.heartbeatPrompt, "")

        let legacyAttachment = try JSONDecoder().decode(
            ProjectAgentMessageAttachment.self,
            from: Data(
                #"{"id":"attachment-1","name":"old.txt","mimeType":"text/plain","size":3,"kind":"file","origin":"file"}"#.utf8
            )
        )
        XCTAssertEqual(legacyAttachment.syncStatus, .localOnly)
        XCTAssertNil(legacyAttachment.sha256)
        XCTAssertNil(legacyAttachment.artifactID)

        let legacyContract = try JSONDecoder().decode(
            LocalAgentTodoExecutionContract.self,
            from: Data("{}".utf8)
        )
        XCTAssertEqual(legacyContract, .init())
    }

    func testAgentAvatarHasBoundedPersistedSize() throws {
        let allowed = LocalAgentProfileDraft(
            name: "Avatar Agent",
            avatarData: Data(repeating: 1, count: 512 * 1_024),
            rolePrompt: "Work",
            modelConfigID: "model-1"
        )
        XCTAssertNoThrow(try allowed.validate())

        let oversized = LocalAgentProfileDraft(
            name: "Avatar Agent",
            avatarData: Data(repeating: 1, count: 512 * 1_024 + 1),
            rolePrompt: "Work",
            modelConfigID: "model-1"
        )
        XCTAssertThrowsError(try oversized.validate())
    }
}
