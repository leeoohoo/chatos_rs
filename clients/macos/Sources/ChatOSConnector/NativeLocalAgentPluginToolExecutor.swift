import ChatOSAgentRuntime
import ChatOSCore
import Foundation

actor NativeLocalAgentPluginToolExecutor {
    private struct Session {
        let ownerUserID: String
        let projectID: String
        let conversationID: String
        let provider: any AgentToolProvider
    }

    private let connector: NativeLocalConnectorService
    private var sessions: [String: Session] = [:]
    private var expirationTasks: [String: Task<Void, Never>] = [:]

    init(connector: NativeLocalConnectorService) {
        self.connector = connector
    }

    func execute(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord,
        context: NativeLocalAgentProjectContext,
        arguments: [String: NativeJSONValue]
    ) async throws -> NativeJSONValue {
        guard NativeAgentCapabilityBrokerToolCatalog.toolNames.contains(invocation.toolName) else {
            throw NativeLocalAgentPlatformToolError.unsupportedTool
        }
        let provider = try await provider(
            ownerUserID: ownerUserID,
            runID: invocation.runID,
            context: context
        )
        refreshExpiration(runID: invocation.runID)
        let outcome = try await provider.execute(.init(
            id: invocation.callID,
            name: invocation.toolName,
            arguments: NativeJSONValue.object(arguments).canonicalJSONString
        ))
        let content: NativeJSONValue
        if let data = outcome.content.data(using: .utf8),
           let decoded = try? JSONDecoder().decode(NativeJSONValue.self, from: data) {
            content = decoded
        } else {
            content = .string(outcome.content)
        }
        return .object([
            "content": content,
            "is_error": .bool(outcome.isError),
            "made_progress": .bool(outcome.madeProgress),
        ])
    }

    func reset() {
        expirationTasks.values.forEach { $0.cancel() }
        expirationTasks.removeAll()
        sessions.removeAll()
    }

    private func provider(
        ownerUserID: String,
        runID: String,
        context: NativeLocalAgentProjectContext
    ) async throws -> any AgentToolProvider {
        if let session = sessions[runID] {
            guard session.ownerUserID == ownerUserID,
                  session.projectID == context.projectID,
                  session.conversationID == context.conversationID else {
                throw NativeLocalAgentPlatformToolError.invalidRunContext
            }
            return session.provider
        }
        let provider = try await connector.makeTaskExecutionCapabilityToolProvider(
            ownerUserID: ownerUserID,
            runID: runID,
            conversationID: context.conversationID,
            projectContext: context.applicationContext
        )
        sessions[runID] = .init(
            ownerUserID: ownerUserID,
            projectID: context.projectID,
            conversationID: context.conversationID,
            provider: provider
        )
        return provider
    }

    private func refreshExpiration(runID: String) {
        expirationTasks.removeValue(forKey: runID)?.cancel()
        expirationTasks[runID] = Task { [weak self] in
            do {
                try await Task.sleep(for: .seconds(300))
            } catch {
                return
            }
            await self?.expire(runID: runID)
        }
    }

    private func expire(runID: String) {
        expirationTasks.removeValue(forKey: runID)?.cancel()
        sessions.removeValue(forKey: runID)
    }
}
