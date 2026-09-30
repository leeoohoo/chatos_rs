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
        let token = try requireAccessToken()
        let configs = try await gateway.modelConfigs(token: token).filter {
            $0.enabled != false && $0.taskEnabled != false && $0.hasAPIKey != false
        }
        let settings = try? await gateway.modelSettings(token: token)
        let credentialStore = NativeLocalAgentModelCredentialStore()
        var environment: [String: String] = [:]
        var snapshots: [LocalAgentModelConfigSnapshot] = []
        var modelOptions: [ConversationModelOption] = []
        for config in configs {
            let resolved = try await gateway.modelConfig(
                token: token,
                id: config.id,
                includeSecret: true
            )
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
            capabilityPolicyRevision: "native-main-chat-v8",
            instructions: "Use local_attachment_read for attachment content. Treat authorized_local_ref values as opaque and never infer or request filesystem paths. Use create_task or create_tasks_with_prerequisites only for user-requested durable work; task state remains local.",
            tools: NativeLocalAgentPlatformToolCatalog.capabilityTools
        )
        try await controlPlane.publishCapabilities(capability)
        try await controlPlane.publishCapabilities(.init(
            ownerUserID: ownerUserID,
            profileKey: "task_execution",
            capabilityPolicyRevision: capability.capabilityPolicyRevision,
            instructions: "Complete the durable local task objective and return a concrete result. Use the local project tools to inspect the bound project. For changes, open an edit session, stage a bounded batch with the read SHA-256 (or null only for a proven-new file), and commit it; the client requests approval before the commit reaches disk. Use execute_command only when project inspection or verification requires it; commands run locally inside the bound project and require approval. For background commands, wait for completion or terminate them before finishing. Requirement surveys are project-bound local records: inspect existing surveys before creating or resolving one, and activate/read the survey skill resources when their detailed contract is needed. Use capability_search only when the task needs an installed Plugin, then describe its opaque option, activate every required Skill, and invoke only a returned tool option. Plugin discovery and execution are account-, project-, and Run-scoped on this client. Do not create nested tasks.",
            tools: NativeLocalAgentPlatformToolCatalog.taskRunnerCapabilityTools
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
