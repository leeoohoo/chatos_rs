import Foundation

/// A scoped tool surface supplied to one Agent run. Providers may be backed by local domain
/// services, MCP processes or remote APIs; AgentRuntime only consumes definitions and outcomes.
public protocol AgentToolProvider: Sendable {
    func definitions() async throws -> [AgentToolDefinition]
    func execute(_ call: AgentToolCall) async throws -> AgentToolOutcome
}

/// Immutable routing table for one run. Duplicate names are rejected before the first model
/// request so a Plugin cannot shadow chat, memory or another Plugin's authority.
public struct AgentToolProviderRegistry: Sendable {
    public let definitions: [AgentToolDefinition]
    private let providersByToolName: [String: any AgentToolProvider]

    public init(providers: [any AgentToolProvider]) async throws {
        var definitions: [AgentToolDefinition] = []
        var providersByToolName: [String: any AgentToolProvider] = [:]
        for provider in providers {
            for definition in try await provider.definitions() {
                guard providersByToolName[definition.name] == nil else {
                    throw AgentRuntimeError.invalidResponse
                }
                definitions.append(definition)
                providersByToolName[definition.name] = provider
            }
        }
        self.definitions = definitions.sorted { $0.name < $1.name }
        self.providersByToolName = providersByToolName
    }

    public func execute(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        guard let provider = providersByToolName[call.name] else {
            return .failure("工具不可用：\(call.name)")
        }
        return try await provider.execute(call)
    }
}
