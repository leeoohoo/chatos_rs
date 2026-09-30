import ChatOSCore
import Foundation

protocol NativeLocalAgentProjectToolExecuting: Sendable {
    func execute(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) async throws -> LocalAgentJSONValue
}

struct NativeLocalAgentProjectContext: Sendable {
    let conversationID: String
    let projectID: String
    let resolvedPath: NativeResolvedProjectPath
}

struct NativeLocalAgentProjectContextResolver: Sendable {
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

    func resolve(
        ownerUserID: String,
        runID: String
    ) async throws -> NativeLocalAgentProjectContext {
        let run = try await runtime.run(ownerUserID: ownerUserID, runID: runID)
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
        let context = try await projects.pluginContext(
            ownerUserID: ownerUserID,
            projectID: resource.resourceID
        )
        guard let projectRoot = context.projectRoot else {
            throw NativeLocalAgentPlatformToolError.projectUnavailable
        }
        let resolvedPath = try await connector.resolveProjectPath(projectRoot)
        return .init(
            conversationID: conversationID,
            projectID: resource.resourceID,
            resolvedPath: resolvedPath
        )
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

struct NativeLocalAgentProjectToolExecutor: NativeLocalAgentProjectToolExecuting, Sendable {
    private let contextResolver: NativeLocalAgentProjectContextResolver
    private let writeStore: NativeMCPCodeWriteStore
    private let terminalStore: NativeLocalAgentTerminalStore

    init(
        host: any LocalAgentHostClientServicing,
        projects: NativeLocalProjectsService,
        connector: NativeLocalConnectorService,
        writeStore: NativeMCPCodeWriteStore = .init(),
        terminalStore: NativeLocalAgentTerminalStore = .init()
    ) {
        contextResolver = .init(host: host, projects: projects, connector: connector)
        self.writeStore = writeStore
        self.terminalStore = terminalStore
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
        let context: NativeLocalAgentProjectContext
        do {
            context = try await contextResolver.resolve(
                ownerUserID: ownerUserID,
                runID: invocation.runID
            )
        } catch let error as NativeLocalAgentPlatformToolError {
            throw error
        } catch {
            throw NativeLocalAgentPlatformToolError.projectUnavailable
        }
        do {
            let tool = NativeMCPCodeReadTools(
                workspace: context.resolvedPath.workspace,
                projectRoot: context.resolvedPath.absoluteURL,
                requestCWD: nil,
                defaultToolRoot: nil
            )
            let nativeArguments = arguments.mapValues(NativeJSONValue.init(local:))
            let result: NativeJSONValue
            if NativeMCPCodeWriteStore.toolNames.contains(invocation.toolName) {
                if invocation.toolName == "commit_edit_session" {
                    guard invocation.requiresApproval,
                          invocation.approvalStatus == "approved" else {
                        throw NativeLocalAgentPlatformToolError.approvalRequired
                    }
                }
                result = try await writeStore.call(
                    name: invocation.toolName,
                    arguments: nativeArguments,
                    scope: .init(
                        workspaceID: context.resolvedPath.workspace.id,
                        sessionID: context.conversationID,
                        runID: invocation.runID
                    ),
                    projectRoot: context.resolvedPath.absoluteURL
                )
            } else if NativeLocalAgentPlatformToolCatalog.taskRunnerTerminalToolNames.contains(
                invocation.toolName
            ) {
                result = try await executeTerminal(
                    invocation: invocation,
                    arguments: nativeArguments,
                    context: context
                )
            } else {
                result = try await Task.detached {
                    try tool.call(name: invocation.toolName, arguments: nativeArguments)
                }.value
            }
            return .init(native: result)
        } catch let error as NativeLocalAgentPlatformToolError {
            throw error
        } catch {
            // Tool and filesystem errors may contain real local paths or staged content.
            // Collapse them before the result crosses IPC and becomes model-visible.
            throw NativeLocalAgentPlatformToolError.projectToolFailed
        }
    }

    func reset() async {
        await terminalStore.cancelAll()
    }

    private func executeTerminal(
        invocation: LocalAgentToolInvocationRecord,
        arguments: [String: NativeJSONValue],
        context: NativeLocalAgentProjectContext
    ) async throws -> NativeJSONValue {
        let name = invocation.toolName
        if ["execute_command", "process_write", "process_kill"].contains(name) {
            guard invocation.requiresApproval,
                  invocation.approvalStatus == "approved" else {
                throw NativeLocalAgentPlatformToolError.approvalRequired
            }
        }
        let root = context.resolvedPath.absoluteURL
        guard name == "execute_command" else {
            return try await terminalStore.call(
                name: name,
                arguments: arguments,
                projectRoot: root,
                ownerRunID: invocation.runID
            )
        }
        let command = Self.string(arguments["common"])
            ?? Self.string(arguments["command"])
            ?? ""
        guard !command.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw NativeLocalAgentPlatformToolError.invalidField("command")
        }
        let cwd = try Self.resolveDirectory(
            Self.string(arguments["path"]) ?? ".",
            projectRoot: root
        )
        let timeout = Self.number(arguments["timeout_ms"]).map(Int.init)
            ?? Self.number(arguments["timeout"]).map { Int($0 * 1_000) }
        return try await terminalStore.execute(
            command: command,
            cwd: cwd,
            projectRoot: root,
            background: Self.bool(arguments["background"]) ?? false,
            timeoutMilliseconds: timeout,
            ownerRunID: invocation.runID
        )
    }

    private static func resolveDirectory(_ path: String, projectRoot: URL) throws -> URL {
        let root = projectRoot.standardizedFileURL.resolvingSymlinksInPath()
        let candidate = path.hasPrefix("/")
            ? URL(fileURLWithPath: path)
            : root.appendingPathComponent(path)
        let resolved = candidate.standardizedFileURL.resolvingSymlinksInPath()
        let prefix = root.path.hasSuffix("/") ? root.path : root.path + "/"
        var isDirectory: ObjCBool = false
        guard (resolved.path == root.path || resolved.path.hasPrefix(prefix)),
              FileManager.default.fileExists(atPath: resolved.path, isDirectory: &isDirectory),
              isDirectory.boolValue else {
            throw NativeLocalAgentPlatformToolError.invalidField("path")
        }
        return resolved
    }

    private static func string(_ value: NativeJSONValue?) -> String? {
        guard case let .string(value)? = value else { return nil }
        return value
    }

    private static func number(_ value: NativeJSONValue?) -> Double? {
        guard case let .number(value)? = value else { return nil }
        return value
    }

    private static func bool(_ value: NativeJSONValue?) -> Bool? {
        guard case let .bool(value)? = value else { return nil }
        return value
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
