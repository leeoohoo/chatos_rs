import ChatOSAgentRuntime
import Foundation

extension NativeLocalConnectorService {
    private static var managedRuntimeConfigTTL: TimeInterval { 5 * 60 }
    private static var managedRuntimeConfigStaleTTL: TimeInterval { 30 * 60 }

    func managedRuntimeConfig() async throws -> GatewayManagedRuntimeConfigDTO {
        try Task.checkCancellation()
        let now = Date()
        if let cache = managedRuntimeConfigCache, cache.expiresAt > now {
            return cache.value
        }

        let generation = managedRuntimeConfigGeneration
        if let refresh = managedRuntimeConfigRefresh, refresh.generation == generation {
            do {
                let value = try await refresh.task.value
                return try finalizeManagedRuntimeConfigRefresh(
                    value,
                    generation: generation
                )
            } catch {
                guard Self.managedRuntimeConfigRefreshIsCurrent(
                    expectedGeneration: generation,
                    currentGeneration: managedRuntimeConfigGeneration,
                    isCancelled: Task.isCancelled
                ) else {
                    throw CancellationError()
                }
                if let cache = managedRuntimeConfigCache, cache.staleUntil > Date() {
                    return cache.value
                }
                throw error
            }
        }

        let token = try requireAccessToken()
        let gateway = gateway
        let task = Task { try await gateway.managedRuntimeConfig(token: token) }
        managedRuntimeConfigRefresh = .init(generation: generation, task: task)
        do {
            let response = try await task.value
            return try finalizeManagedRuntimeConfigRefresh(
                response,
                generation: generation
            )
        } catch {
            if managedRuntimeConfigGeneration == generation {
                managedRuntimeConfigRefresh = nil
            }
            guard Self.managedRuntimeConfigRefreshIsCurrent(
                expectedGeneration: generation,
                currentGeneration: managedRuntimeConfigGeneration,
                isCancelled: Task.isCancelled
            ) else {
                throw CancellationError()
            }
            if let cache = managedRuntimeConfigCache, cache.staleUntil > Date() {
                return cache.value
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

    private func validateManagedRuntimeConfigRefresh(generation: Int) throws {
        guard Self.managedRuntimeConfigRefreshIsCurrent(
            expectedGeneration: generation,
            currentGeneration: managedRuntimeConfigGeneration,
            isCancelled: Task.isCancelled
        ) else {
            throw CancellationError()
        }
    }

    private func finalizeManagedRuntimeConfigRefresh(
        _ response: GatewayManagedRuntimeConfigDTO,
        generation: Int
    ) throws -> GatewayManagedRuntimeConfigDTO {
        try validateManagedRuntimeConfigRefresh(generation: generation)
        let now = Date()
        if let cache = managedRuntimeConfigCache, cache.expiresAt > now {
            if managedRuntimeConfigRefresh?.generation == generation {
                managedRuntimeConfigRefresh = nil
            }
            return cache.value
        }
        let value = try applyManagedRuntimeConfig(response)
        managedRuntimeConfigCache = .init(
            value: value,
            expiresAt: now.addingTimeInterval(Self.managedRuntimeConfigTTL),
            staleUntil: now.addingTimeInterval(Self.managedRuntimeConfigStaleTTL)
        )
        if managedRuntimeConfigRefresh?.generation == generation {
            managedRuntimeConfigRefresh = nil
        }
        return value
    }

    static func managedRuntimeConfigRefreshIsCurrent(
        expectedGeneration: Int,
        currentGeneration: Int,
        isCancelled: Bool
    ) -> Bool {
        !isCancelled && expectedGeneration == currentGeneration
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
        try AgentSettingsStore().saveManagedIfChanged(preferences)
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
