import ChatOSCore
import Foundation

extension NativeLocalConnectorService {
    private static var modelCatalogCacheTTL: TimeInterval { 5 }

    public func fetchModelCatalog(refresh: Bool) async throws -> LocalConnectorModelCatalog {
        let response = try await modelCatalogPayload(forceRefresh: refresh)
        let configs = response.required
        let settings = response.optional
        return .init(
            items: configs.map {
                .init(
                    id: $0.id,
                    sourceProviderID: $0.sourceProviderID,
                    name: $0.name,
                    provider: $0.provider,
                    promptVendor: $0.promptVendor,
                    baseURL: $0.baseURL ?? "",
                    modelName: $0.model,
                    taskUsageScenario: $0.taskUsageScenario,
                    taskThinkingLevel: $0.taskThinkingLevel,
                    temperature: $0.temperature,
                    maxOutputTokens: $0.maxOutputTokens,
                    enabled: $0.enabled ?? true,
                    taskEnabled: $0.taskEnabled ?? ($0.enabled ?? true),
                    hasAPIKey: $0.hasAPIKey ?? true,
                    supportsImages: $0.supportsImages ?? false,
                    supportsReasoning: $0.supportsReasoning ?? false,
                    supportsResponses: $0.supportsResponses ?? false
                )
            },
            settings: .init(
                modelRequestMaxRetries: settings?.modelRequestMaxRetries ?? 5,
                memorySummaryModelConfigID: settings?.memorySummaryModelConfigID,
                memorySummaryThinkingLevel: settings?.memorySummaryThinkingLevel,
                commandApprovalModelConfigID: state.commandApprovalModelConfigID,
                commandApprovalThinkingLevel: state.commandApprovalThinkingLevel
            )
        )
    }

    public func fetchModelProviders() async throws -> [LocalConnectorModelProvider] {
        let token = try requireAccessToken()
        return try await gateway.modelProviders(token: token).map(Self.mapModelProvider)
    }

    public func createModelProvider(_ draft: LocalConnectorModelProviderDraft) async throws {
        let token = try requireAccessToken()
        _ = try await gateway.createModelProvider(token: token, draft: draft)
        invalidateModelCatalog()
    }

    public func updateModelProvider(id: String, draft: LocalConnectorModelProviderDraft) async throws {
        let token = try requireAccessToken()
        _ = try await gateway.updateModelProvider(token: token, id: id, draft: draft)
        invalidateModelCatalog()
    }

    public func refreshModelProvider(id: String) async throws {
        let token = try requireAccessToken()
        _ = try await gateway.refreshModelProvider(token: token, id: id)
        invalidateModelCatalog()
    }

    public func deleteModelProvider(id: String) async throws {
        let token = try requireAccessToken()
        try await gateway.deleteModelProvider(token: token, id: id)
        invalidateModelCatalog()
        if let selected = state.commandApprovalModelConfigID {
            let models = try await gateway.modelConfigs(token: token)
            if !models.contains(where: { $0.id == selected }) {
                state.commandApprovalModelConfigID = nil
                state.commandApprovalThinkingLevel = nil
                try stateStore.save(state)
            }
        }
    }

    public func updateModelConfig(id: String, update: LocalConnectorModelConfigUpdate) async throws {
        let token = try requireAccessToken()
        _ = try await gateway.updateModelConfig(token: token, id: id, update: update)
        invalidateModelCatalog()
        if !update.taskEnabled, state.commandApprovalModelConfigID == id {
            state.commandApprovalModelConfigID = nil
            state.commandApprovalThinkingLevel = nil
            try stateStore.save(state)
        }
    }

    public func updateModelSettings(_ settings: LocalConnectorModelSettings) async throws {
        let token = try requireAccessToken()
        if let approvalID = settings.commandApprovalModelConfigID?.trimmedNonEmpty {
            let model = try await gateway.modelConfig(token: token, id: approvalID, includeSecret: false)
            guard model.enabled ?? true,
                  model.taskEnabled ?? (model.enabled ?? true),
                  model.hasAPIKey ?? false else {
                throw NativeConnectorError.server(
                    status: 409,
                    message: "本机审批 Agent 必须使用已启用且配置了密钥的模型。"
                )
            }
            state.commandApprovalModelConfigID = approvalID
            state.commandApprovalThinkingLevel = settings.commandApprovalThinkingLevel?.trimmedNonEmpty
        } else {
            state.commandApprovalModelConfigID = nil
            state.commandApprovalThinkingLevel = nil
        }
        _ = try await gateway.updateModelSettings(token: token, settings: settings)
        try stateStore.save(state)
        invalidateModelCatalog()
    }

    func modelCatalogPayload(
        forceRefresh: Bool
    ) async throws -> NativeModelCatalogPayload {
        try Task.checkCancellation()
        let now = Date()
        if !forceRefresh,
           let cache = modelCatalogCache,
           Self.modelCatalogCacheIsUsable(
               cacheGeneration: cache.generation,
               currentGeneration: modelCatalogGeneration,
               expiresAt: cache.expiresAt,
               now: now
           ) {
            return cache.value
        }

        if let refresh = modelCatalogRefresh,
           refresh.generation == modelCatalogGeneration {
            do {
                let response = try await refresh.task.value
                return try finalizeModelCatalogRefresh(
                    response,
                    generation: refresh.generation
                )
            } catch {
                if modelCatalogRefresh?.generation == refresh.generation {
                    modelCatalogRefresh = nil
                }
                guard Self.modelCatalogRefreshIsCurrent(
                    expectedGeneration: refresh.generation,
                    currentGeneration: modelCatalogGeneration,
                    isCancelled: Task.isCancelled
                ) else {
                    throw CancellationError()
                }
                throw error
            }
        }

        if forceRefresh {
            modelCatalogGeneration &+= 1
        }
        let generation = modelCatalogGeneration
        let token = try requireAccessToken()
        let gateway = gateway
        let task = Task {
            let response = try await NativeRequiredOptionalParallelLoader.load {
                try await gateway.modelConfigs(token: token)
            } optional: {
                try await gateway.modelSettings(token: token)
            }
            return NativeModelCatalogPayload(
                required: response.required,
                optional: response.optional
            )
        }
        modelCatalogRefresh = .init(generation: generation, task: task)
        do {
            let response = try await task.value
            return try finalizeModelCatalogRefresh(response, generation: generation)
        } catch {
            if modelCatalogRefresh?.generation == generation {
                modelCatalogRefresh = nil
            }
            guard Self.modelCatalogRefreshIsCurrent(
                expectedGeneration: generation,
                currentGeneration: modelCatalogGeneration,
                isCancelled: Task.isCancelled
            ) else {
                throw CancellationError()
            }
            throw error
        }
    }

    func invalidateModelCatalog() {
        modelCatalogGeneration &+= 1
        modelCatalogRefresh?.task.cancel()
        modelCatalogRefresh = nil
        modelCatalogCache = nil
    }

    private func finalizeModelCatalogRefresh(
        _ response: NativeModelCatalogPayload,
        generation: Int
    ) throws -> NativeModelCatalogPayload {
        guard Self.modelCatalogRefreshIsCurrent(
            expectedGeneration: generation,
            currentGeneration: modelCatalogGeneration,
            isCancelled: Task.isCancelled
        ) else {
            throw CancellationError()
        }
        let now = Date()
        if let cache = modelCatalogCache,
           Self.modelCatalogCacheIsUsable(
               cacheGeneration: cache.generation,
               currentGeneration: generation,
               expiresAt: cache.expiresAt,
               now: now
           ) {
            if modelCatalogRefresh?.generation == generation {
                modelCatalogRefresh = nil
            }
            return cache.value
        }
        modelCatalogCache = .init(
            generation: generation,
            value: response,
            expiresAt: now.addingTimeInterval(Self.modelCatalogCacheTTL)
        )
        if modelCatalogRefresh?.generation == generation {
            modelCatalogRefresh = nil
        }
        return response
    }

    static func modelCatalogCacheIsUsable(
        cacheGeneration: Int,
        currentGeneration: Int,
        expiresAt: Date,
        now: Date
    ) -> Bool {
        cacheGeneration == currentGeneration && expiresAt > now
    }

    static func modelCatalogRefreshIsCurrent(
        expectedGeneration: Int,
        currentGeneration: Int,
        isCancelled: Bool
    ) -> Bool {
        !isCancelled && expectedGeneration == currentGeneration
    }

    private static func mapModelProvider(_ provider: GatewayModelProviderDTO) -> LocalConnectorModelProvider {
        .init(
            id: provider.id,
            name: provider.name,
            provider: provider.provider,
            promptVendor: provider.promptVendor ?? provider.provider,
            baseURL: provider.baseURL ?? "",
            hasAPIKey: provider.hasAPIKey ?? false,
            enabled: provider.enabled ?? true,
            supportsImages: provider.supportsImages ?? false,
            supportsReasoning: provider.supportsReasoning ?? false,
            supportsResponses: provider.supportsResponses ?? false,
            lastSyncStatus: provider.lastSyncStatus,
            lastSyncError: provider.lastSyncError,
            importedModelCount: provider.importedModelCount ?? 0
        )
    }
}

struct NativeModelCatalogPayload: Sendable {
    let required: [GatewayModelConfigDTO]
    let optional: GatewayModelSettingsDTO?
}

struct NativeModelCatalogCache: Sendable {
    let generation: Int
    let value: NativeModelCatalogPayload
    let expiresAt: Date
}

struct NativeModelCatalogRefresh: Sendable {
    let generation: Int
    let task: Task<NativeModelCatalogPayload, Error>
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
