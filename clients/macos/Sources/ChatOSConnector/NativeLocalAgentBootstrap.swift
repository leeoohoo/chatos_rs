import CryptoKit
import ChatOSCore
import Foundation

public struct NativeLocalAgentBootstrapResult: Sendable, Equatable {
    public let modelSnapshots: [LocalAgentModelConfigSnapshot]
    public let modelOptions: [ConversationModelOption]
    public let capabilitySnapshot: LocalAgentCapabilityPolicySnapshot
    public let capabilitySnapshotsByModelConfigRef: [String: LocalAgentCapabilityPolicySnapshot]
    public let externalMCPConfigs: [NativeLocalAgentExternalMCPConfig]

    public init(
        modelSnapshots: [LocalAgentModelConfigSnapshot],
        modelOptions: [ConversationModelOption],
        capabilitySnapshot: LocalAgentCapabilityPolicySnapshot,
        capabilitySnapshotsByModelConfigRef: [String: LocalAgentCapabilityPolicySnapshot] = [:],
        externalMCPConfigs: [NativeLocalAgentExternalMCPConfig] = []
    ) {
        self.modelSnapshots = modelSnapshots
        self.modelOptions = modelOptions
        self.capabilitySnapshot = capabilitySnapshot
        self.capabilitySnapshotsByModelConfigRef = capabilitySnapshotsByModelConfigRef.isEmpty
            ? Dictionary(uniqueKeysWithValues: modelSnapshots.map {
                ($0.modelConfigRef, capabilitySnapshot)
            })
            : capabilitySnapshotsByModelConfigRef
        self.externalMCPConfigs = externalMCPConfigs
    }

    public func capabilitySnapshot(
        forModelConfigRef modelConfigRef: String
    ) -> LocalAgentCapabilityPolicySnapshot? {
        capabilitySnapshotsByModelConfigRef[modelConfigRef]
    }
}

extension NativeLocalConnectorService {
    /// Restores the non-secret control plane and Keychain-backed model credentials before
    /// the network catalog refresh completes. This keeps existing local conversations usable
    /// across launch, wake, and temporary gateway outages.
    public func restoreLocalAgentHost(
        _ host: NativeLocalAgentHostLifecycle,
        ownerUserID: String,
        memoryAccessToken: String?
    ) async throws -> NativeLocalAgentBootstrapResult? {
        let controlPlane = NativeLocalAgentControlPlaneClient(host: host)
        let persisted = try await controlPlane.latestModels(ownerUserID: ownerUserID)
        let credentialStore = NativeLocalAgentModelCredentialStore()
        var environment: [String: String] = [:]
        var snapshots: [LocalAgentModelConfigSnapshot] = []
        var modelOptions: [ConversationModelOption] = []
        for snapshot in persisted where snapshot.ownerUserID == ownerUserID {
            let variable = credentialStore.environmentVariable(
                modelConfigRef: snapshot.modelConfigRef
            )
            guard snapshot.credentialRef == "env:\(variable)",
                  environment[variable] == nil,
                  let credential = try credentialStore.loadWithoutUserInteraction(
                    ownerUserID: ownerUserID,
                    modelConfigRef: snapshot.modelConfigRef
                  )?.trimmedNonEmpty else {
                continue
            }
            environment[variable] = credential
            snapshots.append(snapshot)
            let supportsReasoning = snapshot.thinkingLevel?.trimmedNonEmpty != nil
            modelOptions.append(.init(
                id: snapshot.modelConfigRef,
                displayName: snapshot.model,
                modelName: snapshot.model,
                provider: snapshot.provider,
                thinkingLevel: snapshot.thinkingLevel?.trimmedNonEmpty,
                supportsReasoning: supportsReasoning,
                thinkingLevels: supportsReasoning
                    ? Self.thinkingLevels(provider: snapshot.provider)
                    : []
            ))
        }
        guard !snapshots.isEmpty else { return nil }
        if let memoryAccessToken = memoryAccessToken?.trimmedNonEmpty {
            environment["CHATOS_MEMORY_ACCESS_TOKEN"] = memoryAccessToken
        }
        try await host.restart(
            ownerUserID: ownerUserID,
            credentialEnvironment: environment
        )
        environment.removeAll(keepingCapacity: false)
        let capabilitiesByModel = try await restoredMainChatCapabilities(
            controlPlane: controlPlane,
            ownerUserID: ownerUserID,
            modelSnapshots: snapshots
        )
        guard let capability = capabilitiesByModel[snapshots[0].modelConfigRef],
              capability.ownerUserID == ownerUserID,
              capability.profileKey == "main_chat" else { return nil }
        return .init(
            modelSnapshots: snapshots,
            modelOptions: modelOptions,
            capabilitySnapshot: capability,
            capabilitySnapshotsByModelConfigRef: capabilitiesByModel
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
        let promptBundle: GatewayAgentPromptBundleDTO?
        do {
            agentCapability = try await gateway.agentCapability(
                token: token,
                agentKey: "local_agent_execution_agent"
            )
        } catch where Self.shouldUsePersistedCapability(after: error) {
            // The Local Agent control plane is durable client state. A deployment that has not
            // published the optional catalog binding must not disable an already provisioned
            // local task runtime.
            agentCapability = nil
        }
        if let agentCapability {
            guard agentCapability.agentEnabled,
                  agentCapability.ownerUserID == ownerUserID,
                  agentCapability.agentKey == "local_agent_execution_agent" else {
                throw NativeLocalAgentBootstrapError.executionAgentUnavailable
            }
        }
        do {
            promptBundle = try await gateway.agentPromptBundle(token: token)
        } catch where Self.shouldUsePersistedCapability(after: error) {
            promptBundle = nil
        }
        let credentialStore = NativeLocalAgentModelCredentialStore()
        var environment: [String: String] = [:]
        var snapshots: [LocalAgentModelConfigSnapshot] = []
        var modelOptions: [ConversationModelOption] = []
        let resolvedConfigs = try await NativeLocalAgentBoundedLoader.load(
            configs,
            maximumConcurrentTasks: 4
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
            guard environment[variable] == nil else {
                throw NativeLocalAgentBootstrapError.credentialVariableCollision
            }
            // The gateway response is already the authoritative credential. Reading the value
            // back from Keychain can prompt after an app signature change and adds no validation.
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
            let supportsReasoning = resolved.supportsReasoning
                ?? (resolved.taskThinkingLevel?.trimmedNonEmpty != nil)
            modelOptions.append(.init(
                id: resolved.id,
                displayName: resolved.name.trimmedNonEmpty ?? resolved.model,
                modelName: resolved.model,
                provider: resolved.provider,
                thinkingLevel: resolved.taskThinkingLevel?.trimmedNonEmpty,
                supportsReasoning: supportsReasoning,
                thinkingLevels: supportsReasoning
                    ? Self.thinkingLevels(provider: resolved.provider)
                    : []
            ))
        }
        guard !snapshots.isEmpty else { throw NativeLocalAgentBootstrapError.noEnabledModel }
        if let memoryAccessToken = memoryAccessToken?.trimmedNonEmpty {
            environment["CHATOS_MEMORY_ACCESS_TOKEN"] = memoryAccessToken
        }

        try await host.restart(
            ownerUserID: ownerUserID,
            credentialEnvironment: environment
        )
        environment.removeAll(keepingCapacity: false)
        let controlPlane = NativeLocalAgentControlPlaneClient(host: host)
        for snapshot in snapshots {
            try await controlPlane.publishModel(snapshot)
        }
        guard let agentCapability else {
            let capabilitiesByModel: [String: LocalAgentCapabilityPolicySnapshot]
            if let promptBundle {
                var resolvedByID: [String: GatewayModelConfigDTO] = [:]
                for resolved in resolvedConfigs { resolvedByID[resolved.id] = resolved }
                capabilitiesByModel = try await publishManagedPromptsOverPersistedCapabilities(
                    controlPlane: controlPlane,
                    ownerUserID: ownerUserID,
                    modelSnapshots: snapshots,
                    resolvedModelsByID: resolvedByID,
                    promptBundle: promptBundle
                )
            } else {
                capabilitiesByModel = try await restoredMainChatCapabilities(
                    controlPlane: controlPlane,
                    ownerUserID: ownerUserID,
                    modelSnapshots: snapshots
                )
            }
            guard let capability = capabilitiesByModel[snapshots[0].modelConfigRef] else {
                throw NativeLocalAgentBootstrapError.executionAgentUnavailable
            }
            guard capability.ownerUserID == ownerUserID,
                  capability.profileKey == "main_chat" else {
                throw NativeLocalAgentBootstrapError.executionAgentUnavailable
            }
            return .init(
                modelSnapshots: snapshots,
                modelOptions: modelOptions,
                capabilitySnapshot: capability,
                capabilitySnapshotsByModelConfigRef: capabilitiesByModel
            )
        }
        guard let promptBundle else {
            throw NativeLocalAgentBootstrapError.managedPromptUnavailable
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
        let selectablePluginKeys = Set<String>(agentCapability.plugins.compactMap { plugin in
            guard !plugin.binding.required,
                  plugin.available || plugin.status == "partially_available" else { return nil }
            return plugin.catalog.pluginKey
        })
        let pluginChoices = installedPlugins.filter {
            selectablePluginKeys.contains($0.pluginKey)
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
                taskModelChoices.map { .string($0.value) }
            ),
            "enabled_builtin_kinds": .array(requiredBuiltinKinds.map(LocalAgentJSONValue.string)),
            "external_mcp_config_ids": .array(requiredExternalIDs.map(LocalAgentJSONValue.string)),
            "plugin_keys": .array(requiredPluginKeys.map(LocalAgentJSONValue.string)),
        ])
        let requiredPolicyData = try JSONEncoder().encode(requiredPolicy)
        guard let requiredPolicyJSON = String(data: requiredPolicyData, encoding: .utf8) else {
            throw NativeLocalAgentBootstrapError.executionAgentUnavailable
        }
        var resolvedByID: [String: GatewayModelConfigDTO] = [:]
        for resolved in resolvedConfigs { resolvedByID[resolved.id] = resolved }
        var capabilitiesByModel: [String: LocalAgentCapabilityPolicySnapshot] = [:]
        var publishedRevisions = Set<String>()
        for snapshot in snapshots {
            guard let model = resolvedByID[snapshot.modelConfigRef] else {
                throw NativeLocalAgentBootstrapError.managedPromptUnavailable
            }
            let mainPrompt = try NativeManagedAgentPromptResolver.resolve(
                agentKey: "chatos_conversation_agent",
                model: model,
                bundle: promptBundle
            )
            let taskPrompt = try NativeManagedAgentPromptResolver.resolve(
                agentKey: "local_agent_execution_agent",
                model: model,
                bundle: promptBundle
            )
            let capabilityRevision = Self.capabilityRevision(
                agentCapability,
                plugins: pluginChoices,
                builtinChoices: builtinChoices,
                externalChoices: externalChoices,
                taskModelChoices: taskModelChoices,
                promptBundleVersion: promptBundle.bundleVersion,
                mainPrompt: mainPrompt,
                taskPrompt: taskPrompt
            )
            let mainCapability = LocalAgentCapabilityPolicySnapshot(
                ownerUserID: ownerUserID,
                profileKey: "main_chat",
                capabilityPolicyRevision: capabilityRevision,
                instructions: mainPrompt.content,
                tools: NativeLocalAgentPlatformToolCatalog.capabilityTools(
                    pluginChoices: pluginChoices,
                    builtinChoices: builtinChoices,
                    externalChoices: externalChoices,
                    taskModelChoices: taskModelChoices
                )
            )
            capabilitiesByModel[snapshot.modelConfigRef] = mainCapability
            guard publishedRevisions.insert(capabilityRevision).inserted else { continue }
            try await controlPlane.publishCapabilities(mainCapability)
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
                instructions: taskPrompt.content,
                tools: NativeLocalAgentPlatformToolCatalog.taskExecutionCapabilityTools(
                    externalMCPConfigs: externalMCPConfigs
                )
            ))
        }
        guard let capability = capabilitiesByModel[snapshots[0].modelConfigRef] else {
            throw NativeLocalAgentBootstrapError.managedPromptUnavailable
        }
        state.localAgentCapabilityRevisionsByModelConfigID = capabilitiesByModel.mapValues(
            \.capabilityPolicyRevision
        )
        state.localAgentPromptBundleVersion = promptBundle.bundleVersion
        try stateStore.save(state)
        return .init(
            modelSnapshots: snapshots,
            modelOptions: modelOptions,
            capabilitySnapshot: capability,
            capabilitySnapshotsByModelConfigRef: capabilitiesByModel,
            externalMCPConfigs: externalMCPConfigs
        )
    }

    private static func externalMCPConfigs(
        _ mcps: [GatewayResolvedMCPDTO],
        selectedIDs: Set<String>
    ) -> [NativeLocalAgentExternalMCPConfig] {
        var usedToolNames = Set<String>()
        return mcps.sorted { $0.resource.id < $1.resource.id }.compactMap { item in
            guard selectedIDs.contains(item.resource.id),
                  let rawURL = item.resource.runtime.url?.trimmedNonEmpty,
                  let url = URL(string: rawURL) else { return nil }
            let serverName = item.resource.runtime.serverName?.trimmedNonEmpty
                ?? item.resource.name.trimmedNonEmpty
                ?? item.resource.id
            let tools = item.toolSnapshot.compactMap { raw -> NativeLocalAgentExternalMCPTool? in
                guard case .object(let object) = raw,
                      case .string(let upstreamName)? = object["name"],
                      let upstreamName = upstreamName.trimmedNonEmpty else { return nil }
                let description: String
                if case .string(let value)? = object["description"] { description = value }
                else { description = "" }
                let schema = object["inputSchema"] ?? object["input_schema"]
                    ?? .object(["type": .string("object")])
                return .init(
                    publicName: NativeLocalAgentExternalMCPNaming.disambiguatedToolName(
                        server: serverName,
                        tool: upstreamName,
                        resourceID: item.resource.id,
                        used: &usedToolNames
                    ),
                    upstreamName: upstreamName,
                    description: description,
                    inputSchema: schema
                )
            }
            return .init(
                resourceID: item.resource.id,
                serverName: serverName,
                url: url,
                headers: item.resource.runtime.headers,
                tools: tools
            )
        }
    }

    private static func selectableBuiltinChoices(
        _ mcps: [GatewayResolvedMCPDTO]
    ) -> [NativeLocalAgentMCPChoice] {
        let candidates = mcps.filter {
            !$0.binding.required && $0.binding.enabled && $0.resource.enabled
              && $0.resource.runtime.resolvedBuiltinKind != nil
        }
        let available = Set(candidates.compactMap { $0.resource.runtime.resolvedBuiltinKind })
        return candidates.compactMap { item in
            guard let kind = item.resource.runtime.resolvedBuiltinKind else { return nil }
            if kind == "CodeMaintainerWrite", !available.contains("CodeMaintainerRead") {
                return nil
            }
            return .init(value: kind, title: Self.mcpChoiceTitle(item, value: kind))
        }
    }

    private static func selectableExternalChoices(
        _ mcps: [GatewayResolvedMCPDTO]
    ) -> [NativeLocalAgentMCPChoice] {
        mcps.compactMap { item in
            guard !item.binding.required,
                  item.binding.enabled,
                  item.resource.enabled,
                  item.resource.runtime.resolvedBuiltinKind == nil,
                  item.resource.runtime.isExternalHTTP else { return nil }
            return .init(
                value: item.resource.id,
                title: Self.mcpChoiceTitle(item, value: item.resource.id)
            )
        }
    }

    private static func mcpChoiceTitle(
        _ item: GatewayResolvedMCPDTO,
        value: String
    ) -> String {
        let display = item.resource.displayName.trimmedNonEmpty
            ?? item.resource.name.trimmedNonEmpty
            ?? value
        var title = display == value ? value : "\(display) (\(value))"
        if let description = item.resource.description?.trimmedNonEmpty {
            title += " - \(description)"
        }
        let names = item.toolSnapshot.compactMap { tool -> String? in
            guard case .object(let object) = tool,
                  case .string(let name)? = object["name"] else { return nil }
            return name.trimmedNonEmpty
        }.prefix(12)
        if !names.isEmpty { title += " [tools: \(names.joined(separator: ", "))]" }
        return title
    }

    private static func capabilityRevision(
        _ capability: GatewayAgentCapabilityDTO,
        plugins: [NativeInstalledAgentPlugin],
        builtinChoices: [NativeLocalAgentMCPChoice],
        externalChoices: [NativeLocalAgentMCPChoice],
        taskModelChoices: [NativeLocalAgentMCPChoice],
        promptBundleVersion: Int64,
        mainPrompt: GatewayAgentPromptDTO,
        taskPrompt: GatewayAgentPromptDTO
    ) -> String {
        let fields = [managedPromptRevision(
            baseRevision: capability.policyRevision,
            promptBundleVersion: promptBundleVersion,
            mainPrompt: mainPrompt,
            taskPrompt: taskPrompt
        )]
          + plugins.map(\.pluginKey).sorted()
          + builtinChoices.map(\.value).sorted()
          + externalChoices.map(\.value).sorted()
          + taskModelChoices.map(\.value).sorted()
        let digest = SHA256.hash(data: Data(fields.joined(separator: "\u{0}").utf8))
          .map { String(format: "%02x", $0) }.joined()
        return "local-agent-\(digest)"
    }

    private static func managedPromptRevision(
        baseRevision: String,
        promptBundleVersion: Int64,
        mainPrompt: GatewayAgentPromptDTO,
        taskPrompt: GatewayAgentPromptDTO
    ) -> String {
        let fields = [
            baseRevision,
            "prompt-bundle:\(promptBundleVersion)",
            "\(mainPrompt.agentKey):\(mainPrompt.vendor):\(mainPrompt.revision):\(mainPrompt.checksum)",
            "\(taskPrompt.agentKey):\(taskPrompt.vendor):\(taskPrompt.revision):\(taskPrompt.checksum)",
        ]
        let digest = SHA256.hash(data: Data(fields.joined(separator: "\u{0}").utf8))
            .map { String(format: "%02x", $0) }.joined()
        return "managed-prompt-\(digest)"
    }

    static func shouldUsePersistedCapability(after error: Error) -> Bool {
        guard let connectorError = error as? NativeConnectorError,
              case let .server(status, _) = connectorError else { return false }
        return status == 404
    }

    private func publishManagedPromptsOverPersistedCapabilities(
        controlPlane: NativeLocalAgentControlPlaneClient,
        ownerUserID: String,
        modelSnapshots: [LocalAgentModelConfigSnapshot],
        resolvedModelsByID: [String: GatewayModelConfigDTO],
        promptBundle: GatewayAgentPromptBundleDTO
    ) async throws -> [String: LocalAgentCapabilityPolicySnapshot] {
        if state.localAgentPromptBundleVersion == promptBundle.bundleVersion,
           let revisions = state.localAgentCapabilityRevisionsByModelConfigID,
           modelSnapshots.allSatisfy({ revisions[$0.modelConfigRef]?.trimmedNonEmpty != nil }) {
            return try await restoredMainChatCapabilities(
                controlPlane: controlPlane,
                ownerUserID: ownerUserID,
                modelSnapshots: modelSnapshots
            )
        }
        let persisted = try await restoredMainChatCapabilities(
            controlPlane: controlPlane,
            ownerUserID: ownerUserID,
            modelSnapshots: modelSnapshots
        )
        var result: [String: LocalAgentCapabilityPolicySnapshot] = [:]
        var publishedRevisions = Set<String>()
        for snapshot in modelSnapshots {
            guard let model = resolvedModelsByID[snapshot.modelConfigRef],
                  let baseMain = persisted[snapshot.modelConfigRef] else {
                throw NativeLocalAgentBootstrapError.managedPromptUnavailable
            }
            let mainPrompt = try NativeManagedAgentPromptResolver.resolve(
                agentKey: "chatos_conversation_agent",
                model: model,
                bundle: promptBundle
            )
            let taskPrompt = try NativeManagedAgentPromptResolver.resolve(
                agentKey: "local_agent_execution_agent",
                model: model,
                bundle: promptBundle
            )
            let baseTask = try await controlPlane.capabilities(
                ownerUserID: ownerUserID,
                profileKey: "task_execution",
                capabilityPolicyRevision: baseMain.capabilityPolicyRevision
            )
            let basePolicy: LocalAgentCapabilityPolicySnapshot?
            do {
                basePolicy = try await controlPlane.capabilities(
                    ownerUserID: ownerUserID,
                    profileKey: "task_policy_internal",
                    capabilityPolicyRevision: baseMain.capabilityPolicyRevision
                )
            } catch let error as NativeLocalAgentHostError {
                guard case let .hostError(code, _, _) = error, code == "not_found" else {
                    throw error
                }
                basePolicy = nil
            }
            let revision = Self.managedPromptRevision(
                baseRevision: baseMain.capabilityPolicyRevision,
                promptBundleVersion: promptBundle.bundleVersion,
                mainPrompt: mainPrompt,
                taskPrompt: taskPrompt
            )
            let main = LocalAgentCapabilityPolicySnapshot(
                ownerUserID: ownerUserID,
                profileKey: "main_chat",
                capabilityPolicyRevision: revision,
                instructions: mainPrompt.content,
                prefixedInputItems: baseMain.prefixedInputItems,
                tools: baseMain.tools
            )
            result[snapshot.modelConfigRef] = main
            guard publishedRevisions.insert(revision).inserted else { continue }
            try await controlPlane.publishCapabilities(main)
            try await controlPlane.publishCapabilities(.init(
                ownerUserID: ownerUserID,
                profileKey: "task_execution",
                capabilityPolicyRevision: revision,
                instructions: taskPrompt.content,
                prefixedInputItems: baseTask.prefixedInputItems,
                tools: baseTask.tools
            ))
            try await controlPlane.publishCapabilities(.init(
                ownerUserID: ownerUserID,
                profileKey: "task_policy_internal",
                capabilityPolicyRevision: revision,
                instructions: basePolicy?.instructions ?? "{}",
                prefixedInputItems: basePolicy?.prefixedInputItems ?? [],
                tools: basePolicy?.tools ?? []
            ))
        }
        guard result.count == modelSnapshots.count else {
            throw NativeLocalAgentBootstrapError.managedPromptUnavailable
        }
        state.localAgentCapabilityRevisionsByModelConfigID = result.mapValues(
            \.capabilityPolicyRevision
        )
        state.localAgentPromptBundleVersion = promptBundle.bundleVersion
        try stateStore.save(state)
        return result
    }

    private func restoredMainChatCapabilities(
        controlPlane: NativeLocalAgentControlPlaneClient,
        ownerUserID: String,
        modelSnapshots: [LocalAgentModelConfigSnapshot]
    ) async throws -> [String: LocalAgentCapabilityPolicySnapshot] {
        var result: [String: LocalAgentCapabilityPolicySnapshot] = [:]
        var validatedRevisions = Set<String>()
        for snapshot in modelSnapshots {
            guard let revision = state.localAgentCapabilityRevisionsByModelConfigID?[
                snapshot.modelConfigRef
            ]?.trimmedNonEmpty else { continue }
            do {
                let main = try await controlPlane.capabilities(
                    ownerUserID: ownerUserID,
                    profileKey: "main_chat",
                    capabilityPolicyRevision: revision
                )
                if !validatedRevisions.contains(revision) {
                    _ = try await controlPlane.capabilities(
                        ownerUserID: ownerUserID,
                        profileKey: "task_execution",
                        capabilityPolicyRevision: revision
                    )
                    validatedRevisions.insert(revision)
                }
                guard main.ownerUserID == ownerUserID,
                      main.profileKey == "main_chat",
                      main.capabilityPolicyRevision == revision else { continue }
                result[snapshot.modelConfigRef] = main
            } catch {
                continue
            }
        }
        if result.count < modelSnapshots.count {
            let latest = try await controlPlane.latestCapabilities(
                ownerUserID: ownerUserID,
                profileKey: "main_chat"
            )
            _ = try await controlPlane.capabilities(
                ownerUserID: ownerUserID,
                profileKey: "task_execution",
                capabilityPolicyRevision: latest.capabilityPolicyRevision
            )
            for snapshot in modelSnapshots where result[snapshot.modelConfigRef] == nil {
                result[snapshot.modelConfigRef] = latest
            }
        }
        guard result.count == modelSnapshots.count else {
            throw NativeLocalAgentBootstrapError.executionAgentUnavailable
        }
        return result
    }

    private func modelRevision(_ model: GatewayModelConfigDTO) -> String {
        let fields = [
            model.id,
            model.provider,
            model.model,
            model.baseURL ?? "",
            model.taskThinkingLevel ?? "",
            model.temperature.map { String($0) } ?? "",
            model.maxOutputTokens.map { String($0) } ?? "",
            String(model.supportsImages ?? false),
            String(model.supportsResponses ?? false),
        ].joined(separator: "\u{0}")
        let digest = SHA256.hash(data: Data(fields.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
        return "sha256-\(digest)"
    }

    private static func thinkingLevels(provider: String) -> [String] {
        switch provider.trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased().replacingOccurrences(of: "-", with: "_") {
        case "gpt", "openai":
            ["none", "minimal", "low", "medium", "high", "xhigh"]
        case "deepseek":
            ["none", "low", "medium", "high", "max"]
        case "kimi", "kimik2", "moonshot":
            ["none", "auto", "low", "medium", "high", "xhigh"]
        default:
            ["none", "low", "medium", "high", "xhigh"]
        }
    }
}

public enum NativeLocalAgentBootstrapError: LocalizedError {
    case noEnabledModel
    case credentialVariableCollision
    case executionAgentUnavailable
    case managedPromptUnavailable

    public var errorDescription: String? {
        switch self {
        case .noEnabledModel:
            "No enabled Local Agent model with a credential is configured."
        case .credentialVariableCollision:
            "Local Agent model identifiers produce the same credential variable."
        case .executionAgentUnavailable:
            "The Local Agent execution capability is unavailable for this account."
        case .managedPromptUnavailable:
            "The managed Agent Prompt configuration is unavailable."
        }
    }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
