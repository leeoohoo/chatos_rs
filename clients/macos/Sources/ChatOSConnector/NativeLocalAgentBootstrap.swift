import CryptoKit
import ChatOSCore
import Foundation

public struct NativeLocalAgentBootstrapResult: Sendable, Equatable {
    public let modelSnapshots: [LocalAgentModelConfigSnapshot]
    public let modelOptions: [ConversationModelOption]
    public let capabilitySnapshot: LocalAgentCapabilityPolicySnapshot

    public init(
        modelSnapshots: [LocalAgentModelConfigSnapshot],
        modelOptions: [ConversationModelOption],
        capabilitySnapshot: LocalAgentCapabilityPolicySnapshot
    ) {
        self.modelSnapshots = modelSnapshots
        self.modelOptions = modelOptions
        self.capabilitySnapshot = capabilitySnapshot
    }
}

extension NativeLocalConnectorService {
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
            try credentialStore.save(
                credential,
                ownerUserID: ownerUserID,
                modelConfigRef: resolved.id
            )
            let variable = credentialStore.environmentVariable(modelConfigRef: resolved.id)
            guard environment[variable] == nil else {
                throw NativeLocalAgentBootstrapError.credentialVariableCollision
            }
            guard let storedCredential = try credentialStore.load(
                ownerUserID: ownerUserID,
                modelConfigRef: resolved.id
            ) else {
                throw NativeLocalAgentBootstrapError.credentialStoreReadFailed
            }
            environment[variable] = storedCredential
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
        let capability = LocalAgentCapabilityPolicySnapshot(
            ownerUserID: ownerUserID,
            profileKey: "main_chat",
            capabilityPolicyRevision: "native-main-chat-v9",
            instructions: "You are the local Main Chat task planner. Use only the local task tools to inspect, create, query, cancel, and hand off durable work. When a project-bound request requires reading project files or using execution tools, create a task bound to the current conversation/project; never claim the project is unavailable and never ask the user to re-upload an already bound project. Do not read project files, run commands, or execute plugins directly. Task state and execution remain local.",
            tools: NativeLocalAgentPlatformToolCatalog.capabilityTools
        )
        try await controlPlane.publishCapabilities(capability)
        try await controlPlane.publishCapabilities(.init(
            ownerUserID: ownerUserID,
            profileKey: "task_execution",
            capabilityPolicyRevision: capability.capabilityPolicyRevision,
            instructions: "Complete the durable local task objective and return a concrete result. Use the local project tools to inspect the bound project. For changes, open an edit session, stage a bounded batch with the read SHA-256 (or null only for a proven-new file), and commit it; the client requests approval before the commit reaches disk. Use execute_command only when project inspection or verification requires it; commands run locally inside the bound project and require approval. For background commands, wait for completion or terminate them before finishing. Requirement surveys are project-bound local records: inspect existing surveys before creating or resolving one, and activate/read the survey skill resources when their detailed contract is needed. Use capability_search only when the task needs an installed Plugin, then describe its opaque option, activate every required Skill, and invoke only a returned tool option. Plugin discovery and execution are account-, project-, and Run-scoped on this client. Do not create nested tasks.",
            tools: NativeLocalAgentPlatformToolCatalog.taskExecutionCapabilityTools
        ))
        return .init(
            modelSnapshots: snapshots,
            modelOptions: modelOptions,
            capabilitySnapshot: capability
        )
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
    case credentialStoreReadFailed

    public var errorDescription: String? {
        switch self {
        case .noEnabledModel:
            "No enabled Local Agent model with a credential is configured."
        case .credentialVariableCollision:
            "Local Agent model identifiers produce the same credential variable."
        case .credentialStoreReadFailed:
            "Local Agent model credential could not be reloaded from Keychain."
        }
    }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
