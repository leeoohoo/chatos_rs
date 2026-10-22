import ChatOSCore
import ChatOSModelCapabilities
import Foundation

public struct NativeLocalAgentBootstrapResult: Sendable, Equatable {
    public let modelSnapshots: [LocalAgentModelConfigSnapshot]
    public let modelOptions: [ConversationModelOption]
    public let defaultConversationModelConfigRef: String
    public let capabilitySnapshot: LocalAgentCapabilityPolicySnapshot
    public let capabilitySnapshotsByModelConfigRef: [String: LocalAgentCapabilityPolicySnapshot]
    public let pluginInstallerCapabilitySnapshotsByModelConfigRef: [
        String: LocalAgentCapabilityPolicySnapshot
    ]
    public let externalMCPConfigs: [NativeLocalAgentExternalMCPConfig]

    public init(
        modelSnapshots: [LocalAgentModelConfigSnapshot],
        modelOptions: [ConversationModelOption],
        defaultConversationModelConfigRef: String,
        capabilitySnapshot: LocalAgentCapabilityPolicySnapshot,
        capabilitySnapshotsByModelConfigRef: [String: LocalAgentCapabilityPolicySnapshot],
        pluginInstallerCapabilitySnapshotsByModelConfigRef: [
            String: LocalAgentCapabilityPolicySnapshot
        ],
        externalMCPConfigs: [NativeLocalAgentExternalMCPConfig] = []
    ) {
        self.modelSnapshots = modelSnapshots
        self.modelOptions = modelOptions
        self.defaultConversationModelConfigRef = defaultConversationModelConfigRef
        self.capabilitySnapshot = capabilitySnapshot
        self.capabilitySnapshotsByModelConfigRef = capabilitySnapshotsByModelConfigRef
        self.pluginInstallerCapabilitySnapshotsByModelConfigRef =
            pluginInstallerCapabilitySnapshotsByModelConfigRef
        self.externalMCPConfigs = externalMCPConfigs
    }

    public func capabilitySnapshot(
        forModelConfigRef modelConfigRef: String
    ) -> LocalAgentCapabilityPolicySnapshot? {
        capabilitySnapshotsByModelConfigRef[modelConfigRef]
    }

    public func pluginInstallerCapabilitySnapshot(
        forModelConfigRef modelConfigRef: String
    ) -> LocalAgentCapabilityPolicySnapshot? {
        pluginInstallerCapabilitySnapshotsByModelConfigRef[modelConfigRef]
    }
}

struct NativeMainChatTaskSkillSnapshot: Sendable, Equatable {
    let checksum: String
    let instructions: String
    let prefixedInputItem: LocalAgentJSONValue
    let bundle: LocalAgentSkillBundleSnapshot
}

extension NativeLocalConnectorService {
    /// Restores the non-secret control plane and locally stored model credentials before
    /// the network catalog refresh completes. This keeps existing conversations usable
    /// across launch, wake, and temporary gateway outages.
    public func restoreLocalAgentHost(
        _ host: NativeLocalAgentHostLifecycle,
        ownerUserID: String,
        memoryAccessToken: String?
    ) async throws -> NativeLocalAgentBootstrapResult? {
        let controlPlane = NativeLocalAgentControlPlaneClient(host: host)
        let persisted = try await controlPlane.latestModels(ownerUserID: ownerUserID)
        let credentialStore = localAgentModelCredentialStore
        var environment = try NativeLocalAgentCredentialEnvironment.restoredModels(
            persisted, ownerUserID: ownerUserID, store: credentialStore
        )
        var snapshots: [LocalAgentModelConfigSnapshot] = []
        var modelOptions: [ConversationModelOption] = []
        for snapshot in persisted where snapshot.ownerUserID == ownerUserID {
            let variable = credentialStore.environmentVariable(
                modelConfigRef: snapshot.modelConfigRef
            )
            guard snapshot.credentialRef == "env:\(variable)",
                  environment[variable] != nil else {
                continue
            }
            snapshots.append(snapshot)
            let supportsReasoning = ModelReasoningCapabilities.forModel(snapshot.model) != nil
                || snapshot.thinkingLevel?.trimmedNonEmpty != nil
            modelOptions.append(.init(
                id: snapshot.modelConfigRef,
                displayName: snapshot.model,
                modelName: snapshot.model,
                provider: snapshot.provider,
                thinkingLevel: Self.modelThinkingLevel(snapshot.thinkingLevel, model: snapshot.model),
                supportsReasoning: supportsReasoning,
                thinkingLevels: supportsReasoning
                    ? Self.thinkingLevels(provider: snapshot.provider, model: snapshot.model)
                    : []
            ))
        }
        guard !snapshots.isEmpty else { return nil }
        let defaultConversationModelConfigRef = Self.defaultConversationModelConfigRef(
            in: snapshots
        )
        if let memoryAccessToken = memoryAccessToken?.trimmedNonEmpty {
            environment["CHATOS_MEMORY_ACCESS_TOKEN"] = memoryAccessToken
        }
        try await host.refreshCredentials(
            ownerUserID: ownerUserID,
            credentialEnvironment: environment
        )
        environment.removeAll(keepingCapacity: false)
        let capabilitiesByModel = try await publishSkillsOverPersistedCapabilities(
            controlPlane: controlPlane,
            ownerUserID: ownerUserID,
            modelSnapshots: snapshots
        )
        guard let capability = capabilitiesByModel[defaultConversationModelConfigRef],
              capability.ownerUserID == ownerUserID,
              capability.profileKey == "main_chat" else { return nil }
        let installerCapabilitiesByModel = try await publishPluginInstallerCapabilities(
            controlPlane: controlPlane,
            ownerUserID: ownerUserID,
            mainCapabilitiesByModel: capabilitiesByModel
        )
        return .init(
            modelSnapshots: snapshots,
            modelOptions: modelOptions,
            defaultConversationModelConfigRef: defaultConversationModelConfigRef,
            capabilitySnapshot: capability,
            capabilitySnapshotsByModelConfigRef: capabilitiesByModel,
            pluginInstallerCapabilitySnapshotsByModelConfigRef: installerCapabilitiesByModel
        )
    }

    public func bootstrapLocalAgentHost(
        _ host: NativeLocalAgentHostLifecycle,
        ownerUserID: String,
        memoryAccessToken: String?
    ) async throws -> NativeLocalAgentBootstrapResult {
        guard state.user?.id == ownerUserID else {
            throw NativeConnectorError.notPaired
        }
        let catalog = try await modelCatalogPayload(forceRefresh: false)
        let managedRuntime = try await managedRuntimeConfig()
        let configuredTaskMaxIterations = managedRuntime.localTaskExecutionSettings?.maxIterations
            ?? 600
        guard (2...10_000).contains(configuredTaskMaxIterations) else {
            throw NativeLocalAgentBootstrapError.executionAgentUnavailable
        }
        let taskMaxIterations = UInt32(configuredTaskMaxIterations)
        // Main Chat may use every enabled model with a valid credential. `taskEnabled`
        // only controls whether create_task may bind that model to a background Task.
        let configs = catalog.required.filter {
            $0.isSelectable(for: .general)
        }
        let taskEnabledModelConfigIDs = Set(configs.compactMap {
            $0.isSelectable(for: .taskCreation) ? $0.id : nil
        })
        let settings = catalog.optional
        let token = try requireAccessToken()
        let gateway = gateway
        let agentCapability: GatewayAgentCapabilityDTO?
        do {
            agentCapability = try await gateway.agentCapability(
                token: token,
                agentKey: "local_agent_execution_agent"
            )
        } catch where Self.shouldUsePersistedCapability(after: error) {
            // The Local Agent control plane is durable client state. A deployment that has not
            // published the optional catalog binding must not disable an already provisioned
            // task runtime for this account.
            agentCapability = nil
        }
        if let agentCapability {
            guard agentCapability.agentEnabled,
                  agentCapability.ownerUserID == ownerUserID,
                  agentCapability.agentKey == "local_agent_execution_agent" else {
                throw NativeLocalAgentBootstrapError.executionAgentUnavailable
            }
        }
        let credentialStore = localAgentModelCredentialStore
        let controlPlane = NativeLocalAgentControlPlaneClient(host: host)
        let persisted = try await controlPlane.latestModels(ownerUserID: ownerUserID)
        var environment = try NativeLocalAgentCredentialEnvironment.restoredModels(
            persisted, ownerUserID: ownerUserID, store: credentialStore
        )
        var refreshedCredentialVariables: Set<String> = []
        var snapshots: [LocalAgentModelConfigSnapshot] = []
        var modelOptions: [ConversationModelOption] = []
        let resolvedConfigs = try await NativeLocalAgentBoundedLoader.load(
            configs,
            maximumConcurrentTasks: NativeLocalAgentConcurrencyPolicy.adaptiveDefault
        ) { config in
            try await gateway.modelConfig(
                token: token,
                id: config.id,
                includeSecret: true
            )
        }
        let taskModelChoices = resolvedConfigs.compactMap { model -> NativeLocalAgentMCPChoice? in
            guard taskEnabledModelConfigIDs.contains(model.id) else { return nil }
            let displayName = model.name.trimmedNonEmpty ?? model.model
            let usage = model.taskUsageScenario?.trimmedNonEmpty.map { " - \($0)" } ?? ""
            return .init(
                value: model.id,
                title: "\(displayName) (\(model.provider)/\(model.model))\(usage)"
            )
        }
        for resolved in resolvedConfigs {
            guard let credential = resolved.apiKey?.trimmedNonEmpty,
                  let baseURL = resolved.baseURL?.trimmedNonEmpty,
                  let parsedBaseURL = URL(string: baseURL),
                  parsedBaseURL.scheme == "https" || parsedBaseURL.scheme == "http",
                  !resolved.model.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
                continue
            }
            try credentialStore.saveWithoutUserInteraction(
                credential,
                ownerUserID: ownerUserID,
                modelConfigRef: resolved.id
            )
            let variable = credentialStore.environmentVariable(modelConfigRef: resolved.id)
            guard refreshedCredentialVariables.insert(variable).inserted else {
                throw NativeLocalAgentBootstrapError.credentialVariableCollision
            }
            // The gateway response is already the authoritative credential. Reading the value
            // back from the local credential store adds no validation.
            environment[variable] = credential
            let snapshot = LocalAgentModelConfigSnapshot(
                ownerUserID: ownerUserID,
                modelConfigRef: resolved.id,
                modelConfigRevision: modelRevision(resolved),
                credentialRef: "env:\(variable)",
                baseURL: baseURL,
                model: resolved.model,
                provider: resolved.provider,
                supportsResponses: resolved.supportsResponses ?? false,
                supportsImages: resolved.supportsImages,
                temperature: resolved.temperature,
                maxOutputTokens: resolved.maxOutputTokens.map(Int64.init),
                thinkingLevel: resolved.taskThinkingLevel,
                maxTransientRetries: settings?.modelRequestMaxRetries.flatMap {
                    UInt32(exactly: min(20, max(0, $0)))
                }
            )
            snapshots.append(snapshot)
            let supportsReasoning = ModelReasoningCapabilities.forModel(resolved.model) != nil
                || (resolved.supportsReasoning ?? (resolved.taskThinkingLevel?.trimmedNonEmpty != nil))
            modelOptions.append(.init(
                id: resolved.id,
                displayName: resolved.name.trimmedNonEmpty ?? resolved.model,
                modelName: resolved.model,
                provider: resolved.provider,
                thinkingLevel: Self.modelThinkingLevel(resolved.taskThinkingLevel, model: resolved.model),
                supportsReasoning: supportsReasoning,
                thinkingLevels: supportsReasoning
                    ? Self.thinkingLevels(provider: resolved.provider, model: resolved.model)
                    : []
            ))
        }
        guard !snapshots.isEmpty else { throw NativeLocalAgentBootstrapError.noEnabledModel }
        let defaultConversationModelConfigRef = snapshots.first(where: {
            taskEnabledModelConfigIDs.contains($0.modelConfigRef)
        })?.modelConfigRef ?? snapshots[0].modelConfigRef
        if let memoryAccessToken = memoryAccessToken?.trimmedNonEmpty {
            environment["CHATOS_MEMORY_ACCESS_TOKEN"] = memoryAccessToken
        }

        try await host.refreshCredentials(
            ownerUserID: ownerUserID,
            credentialEnvironment: environment
        )
        environment.removeAll(keepingCapacity: false)
        for snapshot in snapshots {
            try await controlPlane.publishModel(snapshot)
        }
        guard let agentCapability else {
            let capabilitiesByModel = try await publishSkillsOverPersistedCapabilities(
                controlPlane: controlPlane,
                ownerUserID: ownerUserID,
                modelSnapshots: snapshots
            )
            guard let capability = capabilitiesByModel[defaultConversationModelConfigRef] else {
                throw NativeLocalAgentBootstrapError.executionAgentUnavailable
            }
            guard capability.ownerUserID == ownerUserID,
                  capability.profileKey == "main_chat" else {
                throw NativeLocalAgentBootstrapError.executionAgentUnavailable
            }
            let installerCapabilitiesByModel = try await publishPluginInstallerCapabilities(
                controlPlane: controlPlane,
                ownerUserID: ownerUserID,
                mainCapabilitiesByModel: capabilitiesByModel
            )
            return .init(
                modelSnapshots: snapshots,
                modelOptions: modelOptions,
                defaultConversationModelConfigRef: defaultConversationModelConfigRef,
                capabilitySnapshot: capability,
                capabilitySnapshotsByModelConfigRef: capabilitiesByModel,
                pluginInstallerCapabilitySnapshotsByModelConfigRef: installerCapabilitiesByModel
            )
        }
        let installedPlugins = try await installedAgentPlugins(ownerUserID: ownerUserID)
        guard !agentCapability.mcps.contains(where: {
            $0.binding.required && !$0.available
        }), !agentCapability.plugins.contains(where: {
            $0.binding.required
              && !$0.available && $0.status != "partially_available"
        }) else {
            throw NativeLocalAgentBootstrapError.executionAgentUnavailable
        }
        let installedPluginKeys = Set(installedPlugins.map(\.pluginKey))
        let requiredPluginKeys = agentCapability.plugins.compactMap { plugin -> String? in
            guard plugin.binding.required,
                  plugin.available || plugin.status == "partially_available" else { return nil }
            return plugin.catalog.pluginKey
        }
        guard requiredPluginKeys.allSatisfy(installedPluginKeys.contains) else {
            throw NativeLocalAgentBootstrapError.executionAgentUnavailable
        }
        let managedSelectablePluginKeys = Set<String>(agentCapability.plugins.compactMap { plugin in
            guard !plugin.binding.required,
                  plugin.available || plugin.status == "partially_available" else { return nil }
            return plugin.catalog.pluginKey
        })
        let selectablePluginKeys = Self.selectablePluginKeys(
            managedKeys: managedSelectablePluginKeys,
            installedPlugins: installedPlugins,
            records: state.installedPluginRecords ?? [:]
        )
        let pluginChoices = installedPlugins.filter {
            selectablePluginKeys.contains($0.pluginKey)
        }
        let policyPluginKeys = selectablePluginKeys.union(requiredPluginKeys)
        let pluginReleaseSnapshots = try installedPlugins.compactMap {
            plugin -> NativeLocalAgentTaskPluginReleaseSnapshot? in
            guard policyPluginKeys.contains(plugin.pluginKey) else { return nil }
            guard let record = state.installedPluginRecords?[plugin.pluginID],
                  (record.pluginKey?.trimmedNonEmpty ?? record.pluginID) == plugin.pluginKey else {
                throw NativeLocalAgentBootstrapError.executionAgentUnavailable
            }
            return .init(
                pluginKey: plugin.pluginKey,
                componentKey: plugin.componentKey,
                pluginID: plugin.pluginID,
                releaseID: record.releaseID,
                version: record.version,
                artifactSHA256: record.artifactSHA256
            )
        }.sorted { ($0.pluginKey, $0.componentKey) < ($1.pluginKey, $1.componentKey) }
        let policyPluginSelectionIDs = Set(installedPlugins.filter {
            policyPluginKeys.contains($0.pluginKey)
        }.map {
            NativeLocalAgentTaskToolAuthorization.pluginSelectionID(
                pluginKey: $0.pluginKey, componentKey: $0.componentKey
            )
        })
        let snapshotSelectionIDs = Set(pluginReleaseSnapshots.map {
            NativeLocalAgentTaskToolAuthorization.pluginSelectionID(
                pluginKey: $0.pluginKey, componentKey: $0.componentKey
            )
        })
        guard snapshotSelectionIDs == policyPluginSelectionIDs else {
            throw NativeLocalAgentBootstrapError.executionAgentUnavailable
        }
        let builtinChoices = Self.selectableBuiltinChoices(agentCapability.mcps)
        let externalChoices = Self.selectableExternalChoices(agentCapability.mcps)
        let requiredBuiltinKinds = agentCapability.mcps.compactMap { item -> String? in
            guard item.binding.required, item.available else { return nil }
            return item.resource.runtime.resolvedBuiltinKind
        }
        let requiredExternalIDs = agentCapability.mcps.compactMap { item -> String? in
            guard item.binding.required,
                  item.available,
                  item.resource.runtime.resolvedBuiltinKind == nil,
                  item.resource.runtime.isExternalHTTP else { return nil }
            return item.resource.id
        }
        let effectiveExternalIDs = Set(externalChoices.map(\.value) + requiredExternalIDs)
        let externalMCPConfigs = Self.externalMCPConfigs(
            agentCapability.mcps,
            selectedIDs: effectiveExternalIDs
        )
        guard Set(externalMCPConfigs.map(\.resourceID)).isSuperset(of: Set(requiredExternalIDs))
        else { throw NativeLocalAgentBootstrapError.executionAgentUnavailable }
        let requiredPolicy = LocalAgentJSONValue.object([
            "max_iterations": .number(Double(max(1, taskMaxIterations))),
            "allowed_model_config_ids": .array(
                taskModelChoices.map(\.value).sorted().map(LocalAgentJSONValue.string)
            ),
            "enabled_builtin_kinds": .array(requiredBuiltinKinds.sorted().map(LocalAgentJSONValue.string)),
            "external_mcp_config_ids": .array(requiredExternalIDs.sorted().map(LocalAgentJSONValue.string)),
            "plugin_components": .array(installedPlugins.filter {
                requiredPluginKeys.contains($0.pluginKey)
            }.map { plugin in
                .object([
                    "plugin_key": .string(plugin.pluginKey),
                    "component_key": .string(plugin.componentKey),
                    "reason": .string("required by Local Agent capability policy"),
                ])
            }),
            "plugin_release_snapshots": .array(pluginReleaseSnapshots.map { snapshot in
                .object([
                    "plugin_key": .string(snapshot.pluginKey),
                    "component_key": .string(snapshot.componentKey),
                    "plugin_id": .string(snapshot.pluginID),
                    "release_id": .string(snapshot.releaseID),
                    "version": .string(snapshot.version),
                    "artifact_sha256": .string(snapshot.artifactSHA256),
                ])
            }),
        ])
        let policyEncoder = JSONEncoder()
        policyEncoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        let requiredPolicyData = try policyEncoder.encode(requiredPolicy)
        guard let requiredPolicyJSON = String(data: requiredPolicyData, encoding: .utf8) else {
            throw NativeLocalAgentBootstrapError.executionAgentUnavailable
        }
        let taskSkill = try Self.mainChatTaskSkillSnapshot()
        let taskExecutionSkill = try Self.taskExecutionSkillSnapshot(externalMCPConfigs: externalMCPConfigs)
        let skillBundleChecksum = try Self.combinedSkillChecksum(
            main: taskSkill, task: taskExecutionSkill
        )
        let skillBundleJSON = try Self.encodedSkillBundle(taskSkill.bundle)
        let taskExecutionSkillBundleJSON = try Self.encodedSkillBundle(
            taskExecutionSkill.bundle
        )
        let mainTools = NativeLocalAgentPlatformToolCatalog.capabilityTools(
            pluginChoices: pluginChoices, builtinChoices: builtinChoices,
            externalChoices: externalChoices, taskModelChoices: taskModelChoices
        )
        let taskTools = NativeLocalAgentPlatformToolCatalog.taskExecutionCapabilityTools(
            externalMCPConfigs: externalMCPConfigs
        )
        let capabilityRevision = try Self.capabilityRevision(
            baseRevision: agentCapability.policyRevision, skillBundleChecksum: skillBundleChecksum,
            mainTools: mainTools, taskTools: taskTools, taskPolicy: requiredPolicy
        )
        var capabilitiesByModel: [String: LocalAgentCapabilityPolicySnapshot] = [:]
        var publishedRevisions = Set<String>()
        for snapshot in snapshots {
            let mainCapability = LocalAgentCapabilityPolicySnapshot(
                ownerUserID: ownerUserID,
                profileKey: "main_chat",
                capabilityPolicyRevision: capabilityRevision,
                instructions: taskSkill.instructions,
                prefixedInputItems: [taskSkill.prefixedInputItem],
                tools: mainTools
            )
            capabilitiesByModel[snapshot.modelConfigRef] = mainCapability
            guard publishedRevisions.insert(capabilityRevision).inserted else { continue }
            try await controlPlane.publishCapabilities(.init(
                ownerUserID: ownerUserID,
                profileKey: "skill_bundle_internal",
                capabilityPolicyRevision: capabilityRevision,
                instructions: skillBundleJSON
            ))
            try await controlPlane.publishCapabilities(.init(
                ownerUserID: ownerUserID,
                profileKey: "task_skill_bundle_internal",
                capabilityPolicyRevision: capabilityRevision,
                instructions: taskExecutionSkillBundleJSON
            ))
            try await controlPlane.publishCapabilities(.init(
                ownerUserID: ownerUserID,
                profileKey: "task_policy_internal",
                capabilityPolicyRevision: capabilityRevision,
                instructions: requiredPolicyJSON
            ))
            try await controlPlane.publishCapabilities(.init(
                ownerUserID: ownerUserID,
                profileKey: "task_execution",
                capabilityPolicyRevision: capabilityRevision,
                instructions: taskExecutionSkill.instructions,
                prefixedInputItems: [taskExecutionSkill.prefixedInputItem],
                tools: taskTools
            ))
            try await controlPlane.publishCapabilities(mainCapability)
        }
        guard let capability = capabilitiesByModel[defaultConversationModelConfigRef] else {
            throw NativeLocalAgentBootstrapError.skillConfigurationUnavailable
        }
        state.localAgentCapabilityRevisionsByModelConfigID = capabilitiesByModel.mapValues(
            \.capabilityPolicyRevision
        )
        // Historical field remains decodable, but remote behavior Prompts no longer own new Runs.
        state.localAgentPromptBundleVersion = nil
        state.localAgentTaskSkillChecksum = skillBundleChecksum
        try stateStore.save(state)
        let installerCapabilitiesByModel = try await publishPluginInstallerCapabilities(
            controlPlane: controlPlane,
            ownerUserID: ownerUserID,
            mainCapabilitiesByModel: capabilitiesByModel
        )
        return .init(
            modelSnapshots: snapshots,
            modelOptions: modelOptions,
            defaultConversationModelConfigRef: defaultConversationModelConfigRef,
            capabilitySnapshot: capability,
            capabilitySnapshotsByModelConfigRef: capabilitiesByModel,
            pluginInstallerCapabilitySnapshotsByModelConfigRef: installerCapabilitiesByModel,
            externalMCPConfigs: externalMCPConfigs
        )
    }

}

public enum NativeLocalAgentBootstrapError: LocalizedError {
    case noEnabledModel
    case credentialVariableCollision
    case executionAgentUnavailable
    case managedPromptUnavailable
    case skillConfigurationUnavailable

    public var errorDescription: String? {
        switch self {
        case .noEnabledModel:
            "No enabled model with a credential is configured."
        case .credentialVariableCollision:
            "Model identifiers produce the same credential variable."
        case .executionAgentUnavailable:
            "The execution capability is unavailable for this account."
        case .managedPromptUnavailable:
            "The managed Agent Prompt configuration is unavailable."
        case .skillConfigurationUnavailable:
            "The run-bound Product Skill configuration is unavailable; refresh capabilities."
        }
    }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
