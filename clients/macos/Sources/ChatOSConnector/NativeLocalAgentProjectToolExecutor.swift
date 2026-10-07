import ChatOSCore
import Foundation

protocol NativeLocalAgentProjectToolExecuting: Sendable {
    func execute(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) async throws -> LocalAgentJSONValue
    func reset() async
}

struct NativeLocalAgentTaskExecutionContext: Sendable {
    let conversationID: String
    let projectID: String?
    let applicationContext: LocalConnectorPluginApplicationContext
    let resolvedPath: NativeResolvedProjectPath?
    let executionRootURL: URL
    let workspaceScopeID: String
    let toolAuthorization: NativeLocalAgentTaskToolAuthorization
    let remoteConnectionID: String?

    init(
        conversationID: String,
        projectID: String?,
        applicationContext: LocalConnectorPluginApplicationContext,
        resolvedPath: NativeResolvedProjectPath?,
        executionRootURL: URL,
        workspaceScopeID: String,
        toolAuthorization: NativeLocalAgentTaskToolAuthorization,
        remoteConnectionID: String? = nil
    ) {
        self.conversationID = conversationID
        self.projectID = projectID
        self.applicationContext = applicationContext
        self.resolvedPath = resolvedPath
        self.executionRootURL = executionRootURL
        self.workspaceScopeID = workspaceScopeID
        self.toolAuthorization = toolAuthorization
        self.remoteConnectionID = remoteConnectionID
    }

    func requireProject() throws -> NativeResolvedProjectPath {
        guard let resolvedPath, projectID != nil else {
            throw NativeLocalAgentPlatformToolError.projectUnavailable
        }
        return resolvedPath
    }
}

struct NativeLocalAgentTaskToolAuthorization: Sendable {
    let requiresExecution: Bool
    let enabledBuiltinKinds: Set<String>
    let pluginKeys: Set<String>
    let isLegacyUnrestricted: Bool

    var pluginsEnabled: Bool { isLegacyUnrestricted || !pluginKeys.isEmpty }

    static func resolve(_ input: [String: LocalAgentJSONValue]) throws -> Self {
        guard case let .object(options)? = input["tool_options"] else {
            return .init(
                requiresExecution: true,
                enabledBuiltinKinds: [],
                pluginKeys: [],
                isLegacyUnrestricted: true
            )
        }
        guard case let .bool(requiresExecution)? = options["requires_execution"],
              case let .array(rawKinds)? = options["enabled_builtin_kinds"] else {
            throw NativeLocalAgentPlatformToolError.invalidRunContext
        }
        let kinds = try rawKinds.map { value -> String in
            guard case let .string(kind) = value else {
                throw NativeLocalAgentPlatformToolError.invalidRunContext
            }
            return kind
        }
        let pluginKeys: Set<String>
        if case let .array(hints)? = options["plugin_hints"] {
            let keys = try hints.map { hint -> String in
                guard case let .object(values) = hint,
                      case let .string(rawKey)? = values["plugin_key"] else {
                    throw NativeLocalAgentPlatformToolError.invalidRunContext
                }
                let key = rawKey.trimmingCharacters(in: .whitespacesAndNewlines)
                guard !key.isEmpty else {
                    throw NativeLocalAgentPlatformToolError.invalidRunContext
                }
                return key
            }
            pluginKeys = Set(keys)
            guard pluginKeys.count == keys.count else {
                throw NativeLocalAgentPlatformToolError.invalidRunContext
            }
        } else {
            pluginKeys = []
        }
        return .init(
            requiresExecution: requiresExecution,
            enabledBuiltinKinds: Set(kinds),
            pluginKeys: pluginKeys,
            isLegacyUnrestricted: false
        )
    }

    func allows(_ toolName: String) -> Bool {
        if isLegacyUnrestricted { return true }
        if ["read_file_raw", "read_file_range", "list_dir", "search_text", "read_file", "search_files"]
            .contains(toolName) {
            return enabledBuiltinKinds.contains("CodeMaintainerRead")
        }
        if NativeMCPCodeWriteStore.toolNames.contains(toolName) {
            return requiresExecution && enabledBuiltinKinds.contains("CodeMaintainerWrite")
        }
        if NativeLocalAgentPlatformToolCatalog.taskExecutionTerminalToolNames.contains(toolName) {
            return requiresExecution && enabledBuiltinKinds.contains("TerminalController")
        }
        if NativeAgentCapabilityBrokerToolCatalog.toolNames.contains(toolName) {
            return pluginsEnabled
        }
        if NativeLocalAgentPlatformToolCatalog.taskExecutionRemoteToolNames.contains(toolName) {
            return enabledBuiltinKinds.contains("RemoteConnectionController")
        }
        return false
    }
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
    ) async throws -> NativeLocalAgentTaskExecutionContext {
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
              let resource = detail.conversation.resource else {
            throw NativeLocalAgentPlatformToolError.invalidRunContext
        }
        let authorization = try NativeLocalAgentTaskToolAuthorization.resolve(input)
        if resource.kind == .contact {
            let root = try await connector.taskExecutionConversationRoot(
                ownerUserID: ownerUserID,
                conversationID: conversationID
            )
            return .init(
                conversationID: conversationID,
                projectID: nil,
                applicationContext: .device,
                resolvedPath: nil,
                executionRootURL: root,
                workspaceScopeID: "contact:" + NativePluginManifestLoader.sha256(
                    ownerUserID + "\n" + conversationID
                ),
                toolAuthorization: authorization,
                remoteConnectionID: Self.string("remote_connection_id", in: input)
            )
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
            applicationContext: context,
            resolvedPath: resolvedPath,
            executionRootURL: resolvedPath.absoluteURL,
            workspaceScopeID: resolvedPath.workspace.id,
            toolAuthorization: authorization,
            remoteConnectionID: Self.string("remote_connection_id", in: input)
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

    private static func string(
        _ key: String,
        in input: [String: LocalAgentJSONValue]
    ) -> String? {
        guard case let .string(value)? = input[key] else { return nil }
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }
}

struct NativeLocalAgentProjectToolExecutor: NativeLocalAgentProjectToolExecuting, Sendable {
    private let contextResolver: NativeLocalAgentProjectContextResolver
    private let writeStore: NativeMCPCodeWriteStore
    private let terminalStore: NativeLocalAgentTerminalStore
    private let pluginTools: NativeLocalAgentPluginToolExecutor
    private let remoteConnections: NativeMCPRemoteConnectionController?

    init(
        host: any LocalAgentHostClientServicing,
        projects: NativeLocalProjectsService,
        connector: NativeLocalConnectorService,
        remoteConnectionProvider: (any NativeRemoteConnectionRuntimeProviding)? = nil,
        writeStore: NativeMCPCodeWriteStore = .init(),
        terminalStore: NativeLocalAgentTerminalStore = .init()
    ) {
        contextResolver = .init(host: host, projects: projects, connector: connector)
        self.writeStore = writeStore
        self.terminalStore = terminalStore
        pluginTools = .init(connector: connector)
        remoteConnections = remoteConnectionProvider.map {
            NativeMCPRemoteConnectionController(provider: $0)
        }
    }

    func execute(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) async throws -> LocalAgentJSONValue {
        guard NativeLocalAgentPlatformToolCatalog.taskExecutionToolNames.contains(
            invocation.toolName
        ), case let .object(arguments) = invocation.arguments else {
            throw NativeLocalAgentPlatformToolError.invalidArguments
        }
        let context: NativeLocalAgentTaskExecutionContext
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
        guard context.toolAuthorization.allows(invocation.toolName) else {
            throw NativeLocalAgentPlatformToolError.capabilityNotSelected
        }
        do {
            let nativeArguments = arguments.mapValues(NativeJSONValue.init(local:))
            if NativeAgentCapabilityBrokerToolCatalog.toolNames.contains(invocation.toolName) {
                let result = try await pluginTools.execute(
                    ownerUserID: ownerUserID,
                    invocation: invocation,
                    context: context,
                    arguments: nativeArguments
                )
                return .init(native: result)
            }
            let resolvedPath = try context.requireProject()
            let tool = NativeMCPCodeReadTools(
                workspace: resolvedPath.workspace,
                projectRoot: resolvedPath.absoluteURL,
                requestCWD: nil,
                defaultToolRoot: nil
            )
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
                        workspaceID: resolvedPath.workspace.id,
                        sessionID: context.conversationID,
                        runID: invocation.runID
                    ),
                    projectRoot: resolvedPath.absoluteURL
                )
            } else if NativeLocalAgentPlatformToolCatalog.taskExecutionTerminalToolNames.contains(
                invocation.toolName
            ) {
                result = try await executeTerminal(
                    invocation: invocation,
                    arguments: nativeArguments,
                    context: context
                )
            } else if NativeLocalAgentPlatformToolCatalog.taskExecutionRemoteToolNames.contains(
                invocation.toolName
            ) {
                guard let remoteConnections,
                      let remoteConnectionID = context.remoteConnectionID else {
                    throw NativeLocalAgentPlatformToolError.projectToolFailed
                }
                let upstreamName = String(invocation.toolName.dropFirst(
                    NativeLocalAgentPlatformToolCatalog.remoteConnectionToolPrefix.count
                ))
                if ["run_command", "upload_file"].contains(upstreamName) {
                    guard invocation.requiresApproval,
                          invocation.approvalStatus == "approved" else {
                        throw NativeLocalAgentPlatformToolError.approvalRequired
                    }
                }
                var boundArguments = nativeArguments
                boundArguments["connection_id"] = .string(remoteConnectionID)
                result = try await remoteConnections.call(
                    name: upstreamName,
                    arguments: boundArguments
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
        await pluginTools.reset()
    }

    private func executeTerminal(
        invocation: LocalAgentToolInvocationRecord,
        arguments: [String: NativeJSONValue],
        context: NativeLocalAgentTaskExecutionContext
    ) async throws -> NativeJSONValue {
        let name = invocation.toolName
        if ["execute_command", "process_write", "process_kill"].contains(name) {
            guard invocation.requiresApproval,
                  invocation.approvalStatus == "approved" else {
                throw NativeLocalAgentPlatformToolError.approvalRequired
            }
        }
        let root = try context.requireProject().absoluteURL
        guard name == "execute_command" else {
            return try await terminalStore.call(
                name: name,
                arguments: arguments,
                projectRoot: root,
                ownerRunID: invocation.runID
            )
        }
        let command = NativeLocalAgentTerminalCommandResolver.resolve(arguments) ?? ""
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
