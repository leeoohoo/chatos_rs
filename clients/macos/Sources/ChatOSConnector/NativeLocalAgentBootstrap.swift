import CryptoKit
import ChatOSCore
import Foundation

public struct NativeLocalAgentBootstrapResult: Sendable, Equatable {
    public let modelSnapshots: [LocalAgentModelConfigSnapshot]
    public let modelOptions: [ConversationModelOption]
    public let capabilitySnapshot: LocalAgentCapabilityPolicySnapshot
    public let externalMCPConfigs: [NativeLocalAgentExternalMCPConfig]

    public init(
        modelSnapshots: [LocalAgentModelConfigSnapshot],
        modelOptions: [ConversationModelOption],
        capabilitySnapshot: LocalAgentCapabilityPolicySnapshot,
        externalMCPConfigs: [NativeLocalAgentExternalMCPConfig] = []
    ) {
        self.modelSnapshots = modelSnapshots
        self.modelOptions = modelOptions
        self.capabilitySnapshot = capabilitySnapshot
        self.externalMCPConfigs = externalMCPConfigs
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
        let capability = try await controlPlane.latestCapabilities(
            ownerUserID: ownerUserID,
            profileKey: "main_chat"
        )
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
        guard !snapshots.isEmpty,
              capability.ownerUserID == ownerUserID,
              capability.profileKey == "main_chat" else { return nil }
        if let memoryAccessToken = memoryAccessToken?.trimmedNonEmpty {
            environment["CHATOS_MEMORY_ACCESS_TOKEN"] = memoryAccessToken
        }
        try await host.restart(
            ownerUserID: ownerUserID,
            credentialEnvironment: environment
        )
        environment.removeAll(keepingCapacity: false)
        return .init(
            modelSnapshots: snapshots,
            modelOptions: modelOptions,
            capabilitySnapshot: capability
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
        let configs = catalog.required.filter {
            $0.enabled != false && $0.taskEnabled != false && $0.hasAPIKey != false
        }
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
            let capability = try await controlPlane.latestCapabilities(
                ownerUserID: ownerUserID,
                profileKey: "main_chat"
            )
            _ = try await controlPlane.latestCapabilities(
                ownerUserID: ownerUserID,
                profileKey: "task_execution"
            )
            guard capability.ownerUserID == ownerUserID,
                  capability.profileKey == "main_chat" else {
                throw NativeLocalAgentBootstrapError.executionAgentUnavailable
            }
            return .init(
                modelSnapshots: snapshots,
                modelOptions: modelOptions,
                capabilitySnapshot: capability
            )
        }
        let installedPlugins = try installedAgentPlugins(ownerUserID: ownerUserID)
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
            return item.resource.runtime.builtinKind?.trimmedNonEmpty
        }
        let requiredExternalIDs = agentCapability.mcps.compactMap { item -> String? in
            guard item.binding.required,
                  item.available,
                  item.resource.runtime.builtinKind?.trimmedNonEmpty == nil,
                  !item.resource.id.hasPrefix("system_mcp_") else { return nil }
            return item.resource.id
        }
        let effectiveExternalIDs = Set(externalChoices.map(\.value) + requiredExternalIDs)
        let externalMCPConfigs = Self.externalMCPConfigs(
            agentCapability.mcps,
            selectedIDs: effectiveExternalIDs
        )
        guard Set(externalMCPConfigs.map(\.resourceID)).isSuperset(of: Set(requiredExternalIDs))
        else { throw NativeLocalAgentBootstrapError.executionAgentUnavailable }
        let capabilityRevision = Self.capabilityRevision(
            agentCapability,
            plugins: pluginChoices,
            builtinChoices: builtinChoices,
            externalChoices: externalChoices
        )
        let capability = LocalAgentCapabilityPolicySnapshot(
            ownerUserID: ownerUserID,
            profileKey: "main_chat",
            capabilityPolicyRevision: capabilityRevision,
            instructions: "You are the local Main Chat task planner. Use only the local task tools to inspect, create, query, cancel, and hand off durable work. When a project-bound request requires reading project files or using execution tools, create a task bound to the current conversation/project; never claim the project is unavailable and never ask the user to re-upload an already bound project. Do not read project files, run commands, or execute plugins directly. Task state and execution remain local.",
            tools: NativeLocalAgentPlatformToolCatalog.capabilityTools(
                pluginChoices: pluginChoices,
                builtinChoices: builtinChoices,
                externalChoices: externalChoices
            )
        )
        try await controlPlane.publishCapabilities(capability)
        let requiredPolicy = LocalAgentJSONValue.object([
            "enabled_builtin_kinds": .array(requiredBuiltinKinds.map(LocalAgentJSONValue.string)),
            "external_mcp_config_ids": .array(requiredExternalIDs.map(LocalAgentJSONValue.string)),
            "plugin_keys": .array(requiredPluginKeys.map(LocalAgentJSONValue.string)),
        ])
        let requiredPolicyData = try JSONEncoder().encode(requiredPolicy)
        guard let requiredPolicyJSON = String(data: requiredPolicyData, encoding: .utf8) else {
            throw NativeLocalAgentBootstrapError.executionAgentUnavailable
        }
        try await controlPlane.publishCapabilities(.init(
            ownerUserID: ownerUserID,
            profileKey: "task_policy_internal",
            capabilityPolicyRevision: capability.capabilityPolicyRevision,
            instructions: requiredPolicyJSON
        ))
        try await controlPlane.publishCapabilities(.init(
            ownerUserID: ownerUserID,
            profileKey: "task_execution",
            capabilityPolicyRevision: capability.capabilityPolicyRevision,
            instructions: "Complete the durable local task objective and return a concrete result. Use the local project tools to inspect the bound project. For changes, open an edit session, stage a bounded batch with the read SHA-256 (or null only for a proven-new file), and commit it; the client requests approval before the commit reaches disk. Use execute_command only when project inspection or verification requires it; commands run locally inside the bound project and require approval. For background commands, wait for completion or terminate them before finishing. Requirement surveys are project-bound local records: inspect existing surveys before creating or resolving one, and activate/read the survey skill resources when their detailed contract is needed. Use capability_search only when the task needs an installed Plugin, then describe its opaque option, activate every required Skill, and invoke only a returned tool option. Plugin discovery and execution are account-, project-, and Run-scoped on this client. Do not create nested tasks.",
            tools: NativeLocalAgentPlatformToolCatalog.taskExecutionCapabilityTools(
                externalMCPConfigs: externalMCPConfigs
            )
        ))
        return .init(
            modelSnapshots: snapshots,
            modelOptions: modelOptions,
            capabilitySnapshot: capability,
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
              && $0.resource.runtime.builtinKind?.trimmedNonEmpty != nil
        }
        let available = Set(candidates.compactMap { $0.resource.runtime.builtinKind?.trimmedNonEmpty })
        return candidates.compactMap { item in
            guard let kind = item.resource.runtime.builtinKind?.trimmedNonEmpty else { return nil }
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
                  item.resource.runtime.builtinKind?.trimmedNonEmpty == nil,
                  !item.resource.id.hasPrefix("system_mcp_"),
                  item.resource.runtime.kind.lowercased() == "http",
                  item.resource.runtime.url?.trimmedNonEmpty != nil else { return nil }
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
        externalChoices: [NativeLocalAgentMCPChoice]
    ) -> String {
        let fields = [capability.policyRevision]
          + plugins.map(\.pluginKey).sorted()
          + builtinChoices.map(\.value).sorted()
          + externalChoices.map(\.value).sorted()
        let digest = SHA256.hash(data: Data(fields.joined(separator: "\u{0}").utf8))
          .map { String(format: "%02x", $0) }.joined()
        return "local-agent-\(digest)"
    }

    static func shouldUsePersistedCapability(after error: Error) -> Bool {
        guard let connectorError = error as? NativeConnectorError,
              case let .server(status, _) = connectorError else { return false }
        return status == 404
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

    public var errorDescription: String? {
        switch self {
        case .noEnabledModel:
            "No enabled Local Agent model with a credential is configured."
        case .credentialVariableCollision:
            "Local Agent model identifiers produce the same credential variable."
        case .executionAgentUnavailable:
            "The Local Agent execution capability is unavailable for this account."
        }
    }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
