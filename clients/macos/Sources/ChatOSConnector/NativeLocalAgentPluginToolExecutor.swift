import ChatOSAgentRuntime
import ChatOSCore
import Foundation

actor NativeLocalAgentPluginToolExecutor {
    private struct Session {
        let ownerUserID: String
        let workspaceScopeID: String
        let conversationID: String
        let provider: any AgentToolProvider
    }

    private let connector: NativeLocalConnectorService
    private let idleExpiration: Duration
    private var sessions: [String: Session] = [:]
    private var expirationTasks: [String: Task<Void, Never>] = [:]

    init(
        connector: NativeLocalConnectorService,
        idleExpiration: Duration = .seconds(300)
    ) {
        self.connector = connector
        self.idleExpiration = idleExpiration
    }

    func execute(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord,
        context: NativeLocalAgentTaskExecutionContext,
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
        // The timeout is an idle-session timeout, not a deadline for an active
        // plugin call. In particular, browser_session_open can legitimately
        // wait for the user to approve attaching to Chrome. Starting the idle
        // timer before execute used to evict the provider while that approval
        // sheet was open; the approved browser session was then destroyed and
        // the next navigation ran against a fresh process with no bound
        // session. Suspend any previous idle timer for the whole call and only
        // arm a new one once the provider becomes idle again.
        suspendExpiration(runID: invocation.runID)
        defer { refreshExpiration(runID: invocation.runID) }
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

    func release(runID: String) async {
        suspendExpiration(runID: runID)
        sessions.removeValue(forKey: runID)
        await connector.cancelAgentPluginTools(runID: runID)
    }

    private func provider(
        ownerUserID: String,
        runID: String,
        context: NativeLocalAgentTaskExecutionContext
    ) async throws -> any AgentToolProvider {
        if let session = sessions[runID] {
            guard session.ownerUserID == ownerUserID,
                  session.workspaceScopeID == context.workspaceScopeID,
                  session.conversationID == context.conversationID else {
                throw NativeLocalAgentPlatformToolError.invalidRunContext
            }
            return session.provider
        }
        let provider = try await connector.makeTaskExecutionCapabilityToolProvider(
            ownerUserID: ownerUserID,
            runID: runID,
            conversationID: context.conversationID,
            projectContext: context.applicationContext,
            executionRootURL: context.executionRootURL,
            workspaceScopeID: context.workspaceScopeID,
            pluginIDs: context.toolAuthorization.isLegacyUnrestricted
                ? nil
                : context.toolAuthorization.pluginKeys.sorted()
        )
        sessions[runID] = .init(
            ownerUserID: ownerUserID,
            workspaceScopeID: context.workspaceScopeID,
            conversationID: context.conversationID,
            provider: provider
        )
        return provider
    }

    private func refreshExpiration(runID: String) {
        suspendExpiration(runID: runID)
        let idleExpiration = idleExpiration
        expirationTasks[runID] = Task { [weak self] in
            do {
                try await Task.sleep(for: idleExpiration)
            } catch {
                return
            }
            await self?.expire(runID: runID)
        }
    }

    private func suspendExpiration(runID: String) {
        expirationTasks.removeValue(forKey: runID)?.cancel()
    }

    private func expire(runID: String) {
        expirationTasks.removeValue(forKey: runID)?.cancel()
        sessions.removeValue(forKey: runID)
    }
}
