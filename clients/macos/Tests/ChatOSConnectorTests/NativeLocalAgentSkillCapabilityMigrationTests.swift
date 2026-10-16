@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentSkillCapabilityMigrationTests: XCTestCase {
    func testLegacyPromptsAreReplacedWithoutChangingPolicyToolsOrOldSnapshots() throws {
        let main = Self.snapshot("main_chat", instructions: "legacy full Main Chat Prompt")
        let task = Self.snapshot("task_execution", instructions: "legacy full Task Prompt")
        let policy = Self.snapshot("task_policy_internal", instructions: Self.policyJSON)
        let migration = try NativeLocalAgentSkillCapabilityMigration.prepare(
            ownerUserID: "user-1", baseMain: main, baseTask: task, basePolicy: policy,
            baseMainBundle: nil, baseTaskBundle: nil
        )
        XCTAssertEqual(migration.publications.count, 5)
        XCTAssertEqual(migration.publications.last?.profileKey, "main_chat")
        XCTAssertNotEqual(migration.main.capabilityPolicyRevision, "legacy-1")
        XCTAssertEqual(migration.main.tools, main.tools + NativeLocalAgentPlatformToolCatalog.mainChatCompanionTools)
        XCTAssertEqual(main.instructions, "legacy full Main Chat Prompt")
        XCTAssertEqual(task.instructions, "legacy full Task Prompt")
        let newTask = try XCTUnwrap(migration.publications.first { $0.profileKey == "task_execution" })
        let newPolicy = try XCTUnwrap(migration.publications.first { $0.profileKey == "task_policy_internal" })
        XCTAssertEqual(newTask.tools, task.tools + [NativeLocalAgentPlatformToolCatalog.attachmentReadTool])
        XCTAssertEqual(newPolicy.instructions, policy.instructions)
        XCTAssertEqual(newPolicy.tools, policy.tools)
        XCTAssertEqual(newPolicy.prefixedInputItems, policy.prefixedInputItems)
        XCTAssertEqual(migration.main.instructions, try NativeLocalConnectorService.mainChatTaskSkillSnapshot().instructions)
        XCTAssertEqual(newTask.instructions, try NativeLocalConnectorService.taskExecutionSkillSnapshot().instructions)
        XCTAssertTrue(migration.main.prefixedInputItems.contains(Self.unrelatedItem))
        XCTAssertEqual(try Self.prepareAgain(migration).publications, [], "migration must be idempotent")
    }

    func testLegacyAttachmentSchemasUpgradeOnNewRevisionWithoutDuplicates() throws {
        let oldTool = LocalAgentJSONValue.object([
            "name": .string("local_attachment_read"), "parameters": .object([
                "properties": .object(["delivery": .object(["enum": .array([.string("workspace"), .string("content")])])]),
            ]),
        ])
        let oldMain = Self.snapshot("main_chat", tools: [oldTool, oldTool])
        let oldTask = Self.snapshot("task_execution", tools: [oldTool, oldTool])
        let migration = try NativeLocalAgentSkillCapabilityMigration.prepare(
            ownerUserID: "user-1", baseMain: oldMain, baseTask: oldTask,
            basePolicy: Self.snapshot("task_policy_internal", instructions: Self.policyJSON),
            baseMainBundle: nil, baseTaskBundle: nil
        )
        let newTask = try XCTUnwrap(migration.publications.first { $0.profileKey == "task_execution" })
        XCTAssertEqual(newTask.tools, [NativeLocalAgentPlatformToolCatalog.attachmentReadTool])
        XCTAssertEqual(oldTask.tools, [oldTool, oldTool])
        XCTAssertEqual(migration.main.tools, NativeLocalAgentPlatformToolCatalog.mainChatCompanionTools)
        XCTAssertEqual(try Self.prepareAgain(migration).publications, [])
    }

    func testDiscoverySchemaUpgradeIsImmutableDeduplicatedAndDoesNotGrantNewTools() throws {
        let old = LocalAgentJSONValue.object(["name": .string("capability_describe"),
            "parameters": .object(["properties": .object(["plugin_option": .object(["type": .string("string")])])])])
        let before = Self.snapshot("task_execution", tools: [old, old])
        let migration = try NativeLocalAgentSkillCapabilityMigration.prepare(
            ownerUserID: "user-1", baseMain: Self.snapshot("main_chat"), baseTask: before,
            basePolicy: Self.snapshot("task_policy_internal", instructions: Self.policyJSON),
            baseMainBundle: nil, baseTaskBundle: nil
        )
        let after = try XCTUnwrap(migration.publications.first { $0.profileKey == "task_execution" })
        let current = try XCTUnwrap(NativeAgentCapabilityBrokerToolCatalog.localAgentCapabilityTools.first {
            guard case let .object(object) = $0 else { return false }
            return object["name"] == .string("capability_describe")
        })
        XCTAssertEqual(before.tools, [old, old])
        XCTAssertEqual(after.tools, [current, NativeLocalAgentPlatformToolCatalog.attachmentReadTool])
        XCTAssertEqual(try Self.prepareAgain(migration).publications, [])
    }

    func testExternalOnlyTaskRetainsItsFrozenFingerprintAndNativeSchema() throws {
        let config = Self.externalConfig
        let taskSkill = try NativeLocalConnectorService.taskExecutionSkillSnapshot(externalMCPConfigs: [config])
        let bundle = Self.snapshot(
            "task_skill_bundle_internal",
            instructions: try NativeLocalConnectorService.encodedSkillBundle(taskSkill.bundle)
        )
        let externalTools = NativeLocalAgentPlatformToolCatalog.taskExecutionCapabilityTools(externalMCPConfigs: [config])
        let task = Self.snapshot("task_execution", tools: externalTools)
        let migration = try NativeLocalAgentSkillCapabilityMigration.prepare(
            ownerUserID: "user-1", baseMain: Self.snapshot("main_chat"), baseTask: task,
            basePolicy: Self.snapshot("task_policy_internal", instructions: Self.policyJSON),
            baseMainBundle: nil, baseTaskBundle: bundle
        )
        let encoded = try XCTUnwrap(migration.publications.first { $0.profileKey == "task_skill_bundle_internal" }?.instructions)
        let migrated = try JSONDecoder().decode(LocalAgentSkillBundleSnapshot.self, from: Data(encoded.utf8))
        let before = try XCTUnwrap(taskSkill.bundle.toolSkillBindings.first { $0.providerID == config.skillProviderID })
        let after = try XCTUnwrap(migrated.toolSkillBindings.first { $0.providerID == config.skillProviderID })
        XCTAssertEqual(after, before)
        XCTAssertEqual(after.bindingID, try config.skillBindingID())
        XCTAssertEqual(migration.publications.first { $0.profileKey == "task_execution" }?.tools, externalTools)
        XCTAssertEqual(try Self.prepareAgain(migration).publications, [])
    }

    func testExternalToolsWithoutTrustedFrozenBindingRequireRefresh() throws {
        let task = Self.snapshot("task_execution", tools: [.object(["name": .string("external_mcp__legacy")])])
        XCTAssertThrowsError(try NativeLocalAgentSkillCapabilityMigration.prepare(
            ownerUserID: "user-1", baseMain: Self.snapshot("main_chat"), baseTask: task,
            basePolicy: nil, baseMainBundle: nil, baseTaskBundle: nil
        ))
    }

    func testCrossOwnerOrCrossRevisionPolicyCannotBeCopied() throws {
        for policy in [
            Self.snapshot("task_policy_internal", owner: "other-user"),
            Self.snapshot("task_policy_internal", revision: "other-revision"),
            Self.snapshot("main_chat"),
        ] {
            XCTAssertThrowsError(try NativeLocalAgentSkillCapabilityMigration.prepare(
                ownerUserID: "user-1", baseMain: Self.snapshot("main_chat"),
                baseTask: Self.snapshot("task_execution"), basePolicy: policy,
                baseMainBundle: nil, baseTaskBundle: nil
            ))
        }
    }

    func testMalformedStoredBundleFailsInsteadOfDiscardingBindings() throws {
        XCTAssertThrowsError(try NativeLocalAgentSkillCapabilityMigration.prepare(
            ownerUserID: "user-1", baseMain: Self.snapshot("main_chat"),
            baseTask: Self.snapshot("task_execution"), basePolicy: nil, baseMainBundle: nil,
            baseTaskBundle: Self.snapshot("task_skill_bundle_internal", instructions: "not JSON")
        ))
    }

    func testUnknownBundleVersionAndInvalidRemoteFingerprintFailClosed() throws {
        let base = try NativeLocalConnectorService.taskExecutionSkillSnapshot(externalMCPConfigs: [Self.externalConfig])
        let validBinding = try XCTUnwrap(base.bundle.toolSkillBindings.first { $0.providerID == Self.externalConfig.skillProviderID })
        let invalidBinding = LocalAgentToolSkillBindingSnapshot(
            providerID: validBinding.providerID, bindingID: "external-mcp:wrong-resource:" + String(repeating: "0", count: 64),
            toolNames: validBinding.toolNames, routerSkillRef: validBinding.routerSkillRef,
            specialistSkillRef: validBinding.specialistSkillRef, activationPolicy: validBinding.activationPolicy
        )
        for bundle in [
            LocalAgentSkillBundleSnapshot(schemaVersion: 99, bundleRevision: base.bundle.bundleRevision,
                documents: base.bundle.documents, toolSkillBindings: base.bundle.toolSkillBindings),
            LocalAgentSkillBundleSnapshot(schemaVersion: 2, bundleRevision: base.bundle.bundleRevision,
                documents: base.bundle.documents, toolSkillBindings: [invalidBinding]),
        ] {
            XCTAssertThrowsError(try NativeLocalAgentSkillCapabilityMigration.prepare(
                ownerUserID: "user-1", baseMain: Self.snapshot("main_chat"),
                baseTask: Self.snapshot("task_execution", tools:
                    NativeLocalAgentPlatformToolCatalog.taskExecutionCapabilityTools(externalMCPConfigs: [Self.externalConfig])),
                basePolicy: nil, baseMainBundle: nil, baseTaskBundle: Self.snapshot("task_skill_bundle_internal",
                    instructions: NativeLocalConnectorService.encodedSkillBundle(bundle))
            ))
        }
    }

    func testOfflinePublicationIsDeduplicatedAndOldRevisionRemainsAvailable() async throws {
        let (connector, host, root) = try Self.fixture()
        defer { try? FileManager.default.removeItem(at: root) }
        let control = NativeLocalAgentControlPlaneClient(host: host)
        let first = try await connector.publishSkillsOverPersistedCapabilities(
            controlPlane: control, ownerUserID: "user-1", modelSnapshots: Self.models
        )
        XCTAssertEqual(first["model-1"], first["model-2"])
        let published = await host.publishedProfiles
        XCTAssertEqual(published, [
            "skill_bundle_internal", "task_skill_bundle_internal", "task_policy_internal",
            "task_execution", "main_chat",
        ])
        let old = try await control.capabilities(
            ownerUserID: "user-1", profileKey: "main_chat", capabilityPolicyRevision: "legacy-1"
        )
        XCTAssertEqual(old.instructions, "legacy behavior")
        let second = try await connector.publishSkillsOverPersistedCapabilities(
            controlPlane: control, ownerUserID: "user-1", modelSnapshots: Self.models
        )
        XCTAssertEqual(first, second)
        let count = await host.publishedProfiles.count
        XCTAssertEqual(count, 5)
        let state = try NativeConnectorStateStore(stateURL: root.appendingPathComponent("state.json")).load()
        XCTAssertNil(state.localAgentPromptBundleVersion)
        XCTAssertEqual(state.localAgentCapabilityRevisionsByModelConfigID?["model-1"], first["model-1"]?.capabilityPolicyRevision)
    }

    func testInterruptedPublicationKeepsPointerAndRetriesSameImmutableRevision() async throws {
        let (connector, host, root) = try Self.fixture()
        defer { try? FileManager.default.removeItem(at: root) }
        await host.failAfterPublications(2)
        let control = NativeLocalAgentControlPlaneClient(host: host)
        do {
            _ = try await connector.publishSkillsOverPersistedCapabilities(
                controlPlane: control, ownerUserID: "user-1", modelSnapshots: Self.models
            )
            XCTFail("injected publication failure was ignored")
        } catch {}
        let failedState = try NativeConnectorStateStore(stateURL: root.appendingPathComponent("state.json")).load()
        XCTAssertEqual(failedState.localAgentCapabilityRevisionsByModelConfigID?["model-1"], "legacy-1")
        XCTAssertEqual(failedState.localAgentPromptBundleVersion, 42)
        let partialProfiles = await host.publishedProfiles
        XCTAssertFalse(partialProfiles.contains("main_chat"))
        await host.failAfterPublications(nil)
        let result = try await connector.publishSkillsOverPersistedCapabilities(
            controlPlane: control, ownerUserID: "user-1", modelSnapshots: Self.models
        )
        XCTAssertNotEqual(result["model-1"]?.capabilityPolicyRevision, "legacy-1")
    }

    private static let unrelatedItem: LocalAgentJSONValue = .object(["type": .string("message"), "content": .array([
        .object(["type": .string("input_text"), "text": .string("unrelated runtime facts")]),
    ])])
    private static let policyJSON = #"{"plugin_keys":["allowed-plugin"],"enabled_builtin_kinds":[],"max_iterations":600}"#
    private static var externalConfig: NativeLocalAgentExternalMCPConfig {
        .init(resourceID: "remote-1", serverName: "Remote", url: URL(string: "https://example.invalid/mcp")!,
              headers: [:], tools: [.init(publicName: "external_mcp__remote_read", upstreamName: "read",
                                         description: "Read remote data", inputSchema: .object(["type": .string("object")]))])
    }
    private static var models: [LocalAgentModelConfigSnapshot] {
        ["model-1", "model-2"].map {
            .init(ownerUserID: "user-1", modelConfigRef: $0, modelConfigRevision: "model-rev",
                  credentialRef: "env:UNUSED_TEST_CREDENTIAL", baseURL: "https://example.invalid",
                  model: "test", provider: "test", supportsResponses: true)
        }
    }
    private static func snapshot(
        _ profile: String, instructions: String = "legacy behavior",
        tools: [LocalAgentJSONValue] = [.object(["name": .string("existing_tool")])],
        owner: String = "user-1", revision: String = "legacy-1"
    ) -> LocalAgentCapabilityPolicySnapshot {
        .init(ownerUserID: owner, profileKey: profile, capabilityPolicyRevision: revision,
              instructions: instructions, prefixedInputItems: [unrelatedItem], tools: tools)
    }
    private static func prepareAgain(
        _ migration: NativeLocalAgentSkillCapabilityMigration
    ) throws -> NativeLocalAgentSkillCapabilityMigration {
        func snapshot(_ profile: String) -> LocalAgentCapabilityPolicySnapshot? {
            migration.publications.first { $0.profileKey == profile }
        }
        return try .prepare(
            ownerUserID: "user-1", baseMain: migration.main,
            baseTask: XCTUnwrap(snapshot("task_execution")), basePolicy: snapshot("task_policy_internal"),
            baseMainBundle: snapshot("skill_bundle_internal"), baseTaskBundle: snapshot("task_skill_bundle_internal")
        )
    }
    private static func fixture() throws -> (NativeLocalConnectorService, SkillMigrationHostStub, URL) {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("skill-migration-\(UUID())")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        var state = NativeConnectorPersistentState.empty
        state.localAgentCapabilityRevisionsByModelConfigID = ["model-1": "legacy-1", "model-2": "legacy-1"]
        state.localAgentPromptBundleVersion = 42
        let url = root.appendingPathComponent("state.json")
        try NativeConnectorStateStore(stateURL: url).save(state)
        let connector = NativeLocalConnectorService(
            configuration: .init(gatewayBaseURL: URL(string: "http://127.0.0.1:1")!, stateURL: url),
            ticketProvider: SkillMigrationTicketStub()
        )
        let host = SkillMigrationHostStub(snapshots: [
            snapshot("main_chat"), snapshot("task_execution"),
            snapshot("task_policy_internal", instructions: policyJSON),
        ])
        return (connector, host, root)
    }
}

private actor SkillMigrationHostStub: LocalAgentHostClientServicing {
    private var snapshots: [String: LocalAgentCapabilityPolicySnapshot] = [:]
    private var failureLimit: Int?
    private(set) var publishedProfiles: [String] = []
    init(snapshots: [LocalAgentCapabilityPolicySnapshot]) {
        for snapshot in snapshots { self.snapshots[Self.key(snapshot.profileKey, snapshot.capabilityPolicyRevision)] = snapshot }
    }
    func start(ownerUserID: String) async throws {}
    func stop() async {}
    func failAfterPublications(_ limit: Int?) { failureLimit = limit }
    private static func key(_ profile: String, _ revision: String) -> String { profile + ":" + revision }
    func request(command: Data) async throws -> Data {
        let input = try XCTUnwrap(JSONSerialization.jsonObject(with: command) as? [String: Any])
        let snapshot: LocalAgentCapabilityPolicySnapshot
        switch input["type"] as? String {
        case "get_capability_policy_snapshot":
            let key = Self.key(try XCTUnwrap(input["profile_key"] as? String),
                               try XCTUnwrap(input["capability_policy_revision"] as? String))
            guard let stored = snapshots[key] else {
                throw NativeLocalAgentHostError.hostError(code: "not_found", message: "missing", retryable: false)
            }
            snapshot = stored
        case "put_capability_policy_snapshot":
            if let limit = failureLimit, publishedProfiles.count >= limit {
                throw NativeLocalAgentHostError.requestTimedOut
            }
            snapshot = try JSONDecoder().decode(LocalAgentCapabilityPolicySnapshot.self,
                from: JSONSerialization.data(withJSONObject: try XCTUnwrap(input["snapshot"])))
            let key = Self.key(snapshot.profileKey, snapshot.capabilityPolicyRevision)
            if let old = snapshots[key], old != snapshot {
                throw NativeLocalAgentHostError.hostError(code: "immutable", message: "snapshot conflict", retryable: false)
            }
            snapshots[key] = snapshot
            publishedProfiles.append(snapshot.profileKey)
        default:
            throw NativeLocalAgentHostError.invalidCommand
        }
        let encoded = try JSONSerialization.jsonObject(with: JSONEncoder().encode(snapshot))
        return try JSONSerialization.data(withJSONObject: ["type": "capability_policy_snapshot", "snapshot": encoded])
    }
}

private struct SkillMigrationTicketStub: LocalConnectorPairingTicketProviding {
    func issueLocalConnectorPairingTicket() async throws -> String { "unused-test-ticket" }
}
