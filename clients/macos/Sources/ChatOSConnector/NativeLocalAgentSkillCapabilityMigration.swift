import Foundation

/// Builds new immutable revisions; never rewrites capabilities referenced by an existing Run.
struct NativeLocalAgentSkillCapabilityMigration: Sendable {
    let main: LocalAgentCapabilityPolicySnapshot
    let publications: [LocalAgentCapabilityPolicySnapshot]
    let checksum: String

    static func prepare(
        ownerUserID: String,
        baseMain: LocalAgentCapabilityPolicySnapshot,
        baseTask: LocalAgentCapabilityPolicySnapshot,
        basePolicy: LocalAgentCapabilityPolicySnapshot?,
        baseMainBundle: LocalAgentCapabilityPolicySnapshot?,
        baseTaskBundle: LocalAgentCapabilityPolicySnapshot?
    ) throws -> Self {
        let expectedProfiles: [(String, LocalAgentCapabilityPolicySnapshot?)] = [
            ("main_chat", baseMain), ("task_execution", baseTask),
            ("task_policy_internal", basePolicy), ("skill_bundle_internal", baseMainBundle),
            ("task_skill_bundle_internal", baseTaskBundle),
        ]
        guard expectedProfiles.allSatisfy({ profile, snapshot in
            snapshot == nil || (snapshot?.ownerUserID == ownerUserID
                && snapshot?.profileKey == profile
                && snapshot?.capabilityPolicyRevision == baseMain.capabilityPolicyRevision)
        }) else { throw NativeLocalAgentBootstrapError.skillConfigurationUnavailable }
        func decodeBundle(_ snapshot: LocalAgentCapabilityPolicySnapshot?) throws -> LocalAgentSkillBundleSnapshot? {
            guard let snapshot else { return nil }
            guard let instructions = snapshot.instructions,
                  let data = instructions.data(using: .utf8) else {
                throw NativeLocalAgentBootstrapError.skillConfigurationUnavailable
            }
            let bundle = try JSONDecoder().decode(LocalAgentSkillBundleSnapshot.self, from: data)
            guard bundle.schemaVersion == 1 || bundle.schemaVersion == 2 else {
                throw NativeLocalAgentBootstrapError.skillConfigurationUnavailable
            }
            return bundle
        }
        let oldMainBundle = try decodeBundle(baseMainBundle)
        let oldTaskBundle = try decodeBundle(baseTaskBundle)
        let mainSkill = try NativeLocalConnectorService.mainChatTaskSkillSnapshot()
        let taskSkill = try NativeLocalConnectorService.taskExecutionSkillSnapshot(
            preservingExternalBindingsFrom: oldTaskBundle
        )
        let externalToolNames = Set(baseTask.tools.compactMap { tool -> String? in
            guard case let .object(object) = tool,
                  case let .string(name)? = object["name"],
                  name.hasPrefix("external_mcp__") else { return nil }
            return name
        })
        let boundExternalNames = Set(taskSkill.bundle.toolSkillBindings
            .filter { $0.providerID.hasPrefix("chatos.external-mcp:") }.flatMap(\.toolNames))
        // Never quietly drop remote fingerprints or manufacture one without a trusted catalog.
        guard externalToolNames == boundExternalNames else {
            throw NativeLocalAgentBootstrapError.skillConfigurationUnavailable
        }
        let checksum = try NativeLocalConnectorService.combinedSkillChecksum(main: mainSkill, task: taskSkill)
        let mainItems = NativeLocalConnectorService.replacingTaskSkillItem(
            in: baseMain.prefixedInputItems, with: mainSkill.prefixedInputItem
        )
        let taskItems = NativeLocalConnectorService.replacingTaskExecutionSkillItem(
            in: baseTask.prefixedInputItems, with: taskSkill.prefixedInputItem
        )
        // Refresh companion and acceptance schemas on a new revision; never mutate old Runs.
        let companionNames = NativeLocalAgentPlatformToolCatalog.mainChatCompanionTools.compactMap { tool -> LocalAgentJSONValue? in
            guard case .object(let value) = tool else { return nil }
            return value["name"]
        }
        let mainTools = NativeAgentCapabilityBrokerToolCatalog.upgradingDiscoverySchemas(baseMain.tools.filter { tool in
            guard case let .object(value) = tool else { return true }
            return !companionNames.contains(value["name"] ?? .null)
        }.map(NativeLocalAgentPlatformToolCatalog.upgradingTaskAcceptanceSchema)
            + NativeLocalAgentPlatformToolCatalog.mainChatCompanionTools)
        let taskTools = NativeAgentCapabilityBrokerToolCatalog.upgradingDiscoverySchemas(
            NativeLocalAgentPlatformToolCatalog.upgradingAttachmentReadSchema(baseTask.tools)
        )
        if baseMain.instructions == mainSkill.instructions,
           baseTask.instructions == taskSkill.instructions,
           oldMainBundle == mainSkill.bundle, oldTaskBundle == taskSkill.bundle,
           baseMain.prefixedInputItems == mainItems, baseTask.prefixedInputItems == taskItems,
           baseMain.tools == mainTools, baseTask.tools == taskTools,
           basePolicy != nil {
            return .init(main: baseMain, publications: [], checksum: checksum)
        }
        let revision = NativeLocalConnectorService.skillPolicyRevision(
            baseRevision: baseMain.capabilityPolicyRevision, skillBundleChecksum: checksum
        )
        let main = LocalAgentCapabilityPolicySnapshot(
            ownerUserID: ownerUserID, profileKey: "main_chat", capabilityPolicyRevision: revision,
            instructions: mainSkill.instructions, prefixedInputItems: mainItems, tools: mainTools
        )
        let publications: [LocalAgentCapabilityPolicySnapshot] = [
            .init(ownerUserID: ownerUserID, profileKey: "skill_bundle_internal",
                  capabilityPolicyRevision: revision,
                  instructions: try NativeLocalConnectorService.encodedSkillBundle(mainSkill.bundle)),
            .init(ownerUserID: ownerUserID, profileKey: "task_skill_bundle_internal",
                  capabilityPolicyRevision: revision,
                  instructions: try NativeLocalConnectorService.encodedSkillBundle(taskSkill.bundle)),
            .init(ownerUserID: ownerUserID, profileKey: "task_policy_internal",
                  capabilityPolicyRevision: revision, instructions: basePolicy?.instructions ?? "{}",
                  prefixedInputItems: basePolicy?.prefixedInputItems ?? [], tools: basePolicy?.tools ?? []),
            .init(ownerUserID: ownerUserID, profileKey: "task_execution",
                  capabilityPolicyRevision: revision, instructions: taskSkill.instructions,
                  prefixedInputItems: taskItems, tools: taskTools),
            // Publish Main Chat last so latest-capability restore cannot select a partial revision.
            main,
        ]
        return .init(main: main, publications: publications, checksum: checksum)
    }
}

extension NativeLocalConnectorService {
    func publishSkillsOverPersistedCapabilities(
        controlPlane: NativeLocalAgentControlPlaneClient,
        ownerUserID: String,
        modelSnapshots: [LocalAgentModelConfigSnapshot]
    ) async throws -> [String: LocalAgentCapabilityPolicySnapshot] {
        let persisted = try await restoredMainChatCapabilities(
            controlPlane: controlPlane, ownerUserID: ownerUserID, modelSnapshots: modelSnapshots
        )
        var result: [String: LocalAgentCapabilityPolicySnapshot] = [:]
        var migrations: [String: NativeLocalAgentSkillCapabilityMigration] = [:]
        for snapshot in modelSnapshots {
            guard let baseMain = persisted[snapshot.modelConfigRef] else {
                throw NativeLocalAgentBootstrapError.executionAgentUnavailable
            }
            if let prepared = migrations[baseMain.capabilityPolicyRevision] {
                result[snapshot.modelConfigRef] = prepared.main
                continue
            }
            let baseTask = try await controlPlane.capabilities(
                ownerUserID: ownerUserID, profileKey: "task_execution",
                capabilityPolicyRevision: baseMain.capabilityPolicyRevision
            )
            func optionalSnapshot(_ profile: String) async throws -> LocalAgentCapabilityPolicySnapshot? {
                do {
                    return try await controlPlane.capabilities(
                        ownerUserID: ownerUserID, profileKey: profile,
                        capabilityPolicyRevision: baseMain.capabilityPolicyRevision
                    )
                } catch let error as NativeLocalAgentHostError {
                    guard case let .hostError(code, _, _) = error, code == "not_found" else { throw error }
                    return nil
                }
            }
            let basePolicy = try await optionalSnapshot("task_policy_internal")
            let mainBundle = try await optionalSnapshot("skill_bundle_internal")
            let taskBundle = try await optionalSnapshot("task_skill_bundle_internal")
            let prepared = try NativeLocalAgentSkillCapabilityMigration.prepare(
                ownerUserID: ownerUserID, baseMain: baseMain, baseTask: baseTask,
                basePolicy: basePolicy, baseMainBundle: mainBundle, baseTaskBundle: taskBundle
            )
            migrations[baseMain.capabilityPolicyRevision] = prepared
            result[snapshot.modelConfigRef] = prepared.main
        }
        // Validate the complete migration before publishing any model's new revision.
        for revision in migrations.keys.sorted() {
            for publication in migrations[revision]?.publications ?? [] {
                try await controlPlane.publishCapabilities(publication)
            }
        }
        state.localAgentCapabilityRevisionsByModelConfigID = result.mapValues(\.capabilityPolicyRevision)
        state.localAgentPromptBundleVersion = nil
        // Models may preserve distinct dynamic External bindings. Cache an aggregate for diagnostics only.
        state.localAgentTaskSkillChecksum = Self.sha256(
            migrations.values.map(\.checksum).sorted().joined(separator: "\n")
        )
        try stateStore.save(state)
        return result
    }
}
