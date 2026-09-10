import Foundation

/// The host resolves account-owned model configuration and memory transport. Secrets stay out of runs.
public protocol AgentServiceProviding: Sendable {
    func makeAgentModel(configID: String, policy: AgentRunPolicy) async throws -> any AgentModelClient
    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing
}
