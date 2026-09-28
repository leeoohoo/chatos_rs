import ChatOSAgentRuntime
import Foundation

extension NativeLocalConnectorService {
    private static var managedRuntimeConfigTTL: TimeInterval { 5 * 60 }
    private static var managedRuntimeConfigStaleTTL: TimeInterval { 30 * 60 }

    func managedRuntimeConfig() async throws -> GatewayManagedRuntimeConfigDTO {
        let now = Date()
        if let cache = managedRuntimeConfigCache, cache.expiresAt > now {
            return try applyManagedRuntimeConfig(cache.value)
        }

        let generation = managedRuntimeConfigGeneration
        if let refresh = managedRuntimeConfigRefresh, refresh.generation == generation {
            do {
                return try applyManagedRuntimeConfig(await refresh.task.value)
            } catch {
                if let cache = managedRuntimeConfigCache, cache.staleUntil > Date() {
                    return try applyManagedRuntimeConfig(cache.value)
                }
                throw error
            }
        }

        let token = try requireAccessToken()
        let gateway = gateway
        let task = Task { try await gateway.managedRuntimeConfig(token: token) }
        managedRuntimeConfigRefresh = .init(generation: generation, task: task)
        do {
            let value = try applyManagedRuntimeConfig(await task.value)
            if managedRuntimeConfigGeneration == generation {
                let refreshedAt = Date()
                managedRuntimeConfigCache = .init(
                    value: value,
                    expiresAt: refreshedAt.addingTimeInterval(Self.managedRuntimeConfigTTL),
                    staleUntil: refreshedAt.addingTimeInterval(Self.managedRuntimeConfigStaleTTL)
                )
                managedRuntimeConfigRefresh = nil
            }
            return value
        } catch {
            if managedRuntimeConfigGeneration == generation {
                managedRuntimeConfigRefresh = nil
            }
            if let cache = managedRuntimeConfigCache, cache.staleUntil > Date() {
                return try applyManagedRuntimeConfig(cache.value)
            }
            throw error
        }
    }

    func invalidateManagedRuntimeConfig() {
        managedRuntimeConfigGeneration &+= 1
        managedRuntimeConfigRefresh?.task.cancel()
        managedRuntimeConfigRefresh = nil
        managedRuntimeConfigCache = nil
    }

    private func applyManagedRuntimeConfig(
        _ value: GatewayManagedRuntimeConfigDTO
    ) throws -> GatewayManagedRuntimeConfigDTO {
        guard let managed = value.nativeAgentRuntimeSettings else { return value }
        var preferences = AgentRuntimePreferences()
        preferences.global.maximumModelCalls = managed.maximumModelCalls
        preferences.global.maximumRequestRetries = managed.maximumRequestRetries
        preferences.global.requestTimeoutSeconds = managed.requestTimeoutSeconds
        preferences.global.runTimeoutSeconds = managed.runTimeoutSeconds
        preferences.global.maximumNoProgressRounds = managed.maximumNoProgressRounds
        var context = AgentContextPolicy()
        context.windowTokens = managed.contextWindowTokens
        context.outputReserveTokens = managed.outputReserveTokens
        preferences.global.context = context
        try AgentSettingsStore().saveManaged(preferences)
        return value
    }
}

struct NativeManagedRuntimeConfigCache: Sendable {
    let value: GatewayManagedRuntimeConfigDTO
    let expiresAt: Date
    let staleUntil: Date
}

struct NativeManagedRuntimeConfigRefresh: Sendable {
    let generation: Int
    let task: Task<GatewayManagedRuntimeConfigDTO, Error>
}
