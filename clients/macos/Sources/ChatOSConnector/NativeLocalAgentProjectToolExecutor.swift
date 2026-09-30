import ChatOSCore
import Foundation

protocol NativeLocalAgentProjectToolExecuting: Sendable {
    func execute(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) async throws -> LocalAgentJSONValue
}

struct NativeLocalAgentProjectToolExecutor: NativeLocalAgentProjectToolExecuting, Sendable {
    private let runtime: NativeLocalAgentRuntimeClient
    private let conversations: NativeLocalAgentConversationClient
    private let projects: NativeLocalProjectsService
    private let connector: NativeLocalConnectorService

    init(
        host: any LocalAgentHostClientServicing,
        projects: NativeLocalProjectsService,
        connector: NativeLocalConnectorService
    ) {
        runtime = .init(host: host)
        conversations = .init(host: host)
        self.projects = projects
        self.connector = connector
    }

    func execute(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) async throws -> LocalAgentJSONValue {
        guard NativeLocalAgentPlatformToolCatalog.taskRunnerToolNames.contains(
            invocation.toolName
        ), case let .object(arguments) = invocation.arguments else {
            throw NativeLocalAgentPlatformToolError.invalidArguments
        }
        let run = try await runtime.run(ownerUserID: ownerUserID, runID: invocation.runID)
        guard run.ownerUserID == ownerUserID,
              case let .object(input) = run.input,
              let conversationID = Self.conversationID(input) else {
            throw NativeLocalAgentPlatformToolError.invalidRunContext
        }
        let detail = try await conversations.get(
            ownerUserID: ownerUserID,
            conversationID: conversationID
        )
        guard detail.conversation.ownerUserID == ownerUserID,
              let resource = detail.conversation.resource,
              resource.kind == .project else {
            throw NativeLocalAgentPlatformToolError.projectUnavailable
        }

        do {
            let context = try await projects.pluginContext(
                ownerUserID: ownerUserID,
                projectID: resource.resourceID
            )
            guard let projectRoot = context.projectRoot else {
                throw NativeLocalAgentPlatformToolError.projectUnavailable
            }
            let resolved = try await connector.resolveProjectPath(projectRoot)
            let tool = NativeMCPCodeReadTools(
                workspace: resolved.workspace,
                projectRoot: resolved.absoluteURL,
                requestCWD: nil,
                defaultToolRoot: nil
            )
            let nativeArguments = arguments.mapValues(NativeJSONValue.init(local:))
            return try await Task.detached {
                LocalAgentJSONValue(native: try tool.call(
                    name: invocation.toolName,
                    arguments: nativeArguments
                ))
            }.value
        } catch let error as NativeLocalAgentPlatformToolError {
            throw error
        } catch {
            // Project registry, connector, and filesystem errors may contain real local
            // paths. Collapse them before the result crosses IPC and becomes model-visible.
            throw NativeLocalAgentPlatformToolError.projectUnavailable
        }
    }

    private static func conversationID(
        _ input: [String: LocalAgentJSONValue]
    ) -> String? {
        for key in ["source_conversation_id", "conversation_id"] {
            if case let .string(value)? = input[key], !value.isEmpty { return value }
        }
        return nil
    }
}

extension LocalAgentJSONValue {
    init(native value: NativeJSONValue) {
        switch value {
        case .null: self = .null
        case let .bool(value): self = .bool(value)
        case let .number(value): self = .number(value)
        case let .string(value): self = .string(value)
        case let .array(values): self = .array(values.map(Self.init(native:)))
        case let .object(values): self = .object(values.mapValues(Self.init(native:)))
        }
    }
}

extension NativeJSONValue {
    init(local value: LocalAgentJSONValue) {
        switch value {
        case .null: self = .null
        case let .bool(value): self = .bool(value)
        case let .number(value): self = .number(value)
        case let .string(value): self = .string(value)
        case let .array(values): self = .array(values.map(Self.init(local:)))
        case let .object(values): self = .object(values.mapValues(Self.init(local:)))
        }
    }
}
