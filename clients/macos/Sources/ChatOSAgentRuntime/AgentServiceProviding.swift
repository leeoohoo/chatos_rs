import Foundation

/// The host resolves account-owned model configuration and memory transport. Secrets stay out of runs.
public protocol AgentServiceProviding: Sendable {
    func makeAgentModel(configID: String, policy: AgentRunPolicy) async throws -> any AgentModelClient
    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy,
        thinkingLevel: String?
    ) async throws -> any AgentModelClient
    func makeAgentMemory(scope: AgentMemoryScope) async throws -> any AgentMemoryServicing
}

public extension AgentServiceProviding {
    func makeAgentModel(
        configID: String,
        policy: AgentRunPolicy,
        thinkingLevel: String?
    ) async throws -> any AgentModelClient {
        try await makeAgentModel(configID: configID, policy: policy)
    }
}
