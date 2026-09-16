import Foundation

/// A scoped tool surface supplied to one Agent run. Providers may be backed by local domain
/// services, MCP processes or remote APIs; AgentRuntime only consumes definitions and outcomes.
public protocol AgentToolProvider: Sendable {
    func definitions() async throws -> [AgentToolDefinition]
    func execute(_ call: AgentToolCall) async throws -> AgentToolOutcome
}
