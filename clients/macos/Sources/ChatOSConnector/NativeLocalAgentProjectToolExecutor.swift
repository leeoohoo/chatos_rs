import ChatOSCodeReadWorkerClient
import ChatOSCore
import Foundation

protocol NativeLocalAgentProjectToolExecuting: Sendable {
    func execute(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) async throws -> LocalAgentJSONValue
    func release(runID: String) async
    func reset() async
}

extension NativeLocalAgentProjectToolExecuting {
    func release(runID _: String) async {}
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

    var executionWorkspace: LocalConnectorWorkspace {
        if let resolvedPath { return resolvedPath.workspace }
        return .init(
            id: workspaceScopeID,
            alias: "Conversation Workspace",
            absoluteRoot: executionRootURL.path,
            fingerprint: NativePluginManifestLoader.sha256(
                workspaceScopeID + "\n" + executionRootURL.path
            )
        )
    }
}

struct NativeLocalAgentTaskToolAuthorization: Sendable {
    let requiresExecution: Bool
    let enabledBuiltinKinds: Set<String>
    let pluginSelectionIDs: Set<String>
    let pluginReleaseSnapshots: [String: NativeLocalAgentTaskPluginReleaseSnapshot]
    let ownedPaths: [String]
    let externalMCPConfigIDs: Set<String>

    init(
        requiresExecution: Bool,
        enabledBuiltinKinds: Set<String>,
        pluginSelectionIDs: Set<String>,
        pluginReleaseSnapshots: [String: NativeLocalAgentTaskPluginReleaseSnapshot] = [:],
        ownedPaths: [String] = [],
        externalMCPConfigIDs: Set<String> = []
    ) {
        self.requiresExecution = requiresExecution
        self.enabledBuiltinKinds = enabledBuiltinKinds
        self.pluginSelectionIDs = pluginSelectionIDs
        self.pluginReleaseSnapshots = pluginReleaseSnapshots
        self.ownedPaths = ownedPaths
        self.externalMCPConfigIDs = externalMCPConfigIDs
    }

    var capabilityBrokerEnabled: Bool {
        !pluginSelectionIDs.isEmpty || !brokerBuiltinCapabilities.isEmpty || !externalMCPConfigIDs.isEmpty
    }

    var brokerBuiltinCapabilities: Set<LocalAgentTodoBuiltinCapability> {
        var capabilities: Set<LocalAgentTodoBuiltinCapability> = []
        if enabledBuiltinKinds.contains("CodeMaintainerRead") {
            capabilities.insert(.projectRead)
        }
        if requiresExecution, enabledBuiltinKinds.contains("CodeMaintainerWrite") {
            capabilities.insert(.projectWrite)
        }
        if requiresExecution, enabledBuiltinKinds.contains("TerminalController") {
            capabilities.insert(.terminal)
        }
        return capabilities
    }

    static func resolve(_ input: [String: LocalAgentJSONValue]) throws -> Self {
        let ownedPaths: [String]
        if let raw = input["owned_paths"] {
            guard case let .array(values) = raw else {
                throw NativeLocalAgentPlatformToolError.invalidRunContext
            }
            let paths = try values.map { value -> String in
                guard case let .string(path) = value else {
                    throw NativeLocalAgentPlatformToolError.invalidRunContext
                }
                return path
            }
            guard let validated = try? NativeMCPCodeWriteOwnership.validate(paths) else {
                throw NativeLocalAgentPlatformToolError.invalidRunContext
            }
            ownedPaths = validated
        } else {
            ownedPaths = []
        }
        guard case let .object(options)? = input["tool_options"] else {
            throw NativeLocalAgentPlatformToolError.invalidRunContext
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
        guard case let .array(hints)? = options["plugin_hints"] else {
            throw NativeLocalAgentPlatformToolError.invalidRunContext
        }
        let selectionIDs = try hints.map { hint -> String in
            guard case let .object(values) = hint,
                  case let .string(rawKey)? = values["plugin_key"],
                  case let .string(rawComponentKey)? = values["component_key"] else {
                throw NativeLocalAgentPlatformToolError.invalidRunContext
            }
            let key = rawKey.trimmingCharacters(in: .whitespacesAndNewlines)
            let componentKey = rawComponentKey.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !key.isEmpty, !componentKey.isEmpty else {
                throw NativeLocalAgentPlatformToolError.invalidRunContext
            }
            return Self.pluginSelectionID(pluginKey: key, componentKey: componentKey)
        }
        let pluginSelectionIDs = Set(selectionIDs)
        guard pluginSelectionIDs.count == selectionIDs.count else {
            throw NativeLocalAgentPlatformToolError.invalidRunContext
        }
        guard case let .array(rawSnapshots)? = options["plugin_release_snapshots"] else {
            throw NativeLocalAgentPlatformToolError.invalidRunContext
        }
        let snapshots = try rawSnapshots.map(NativeLocalAgentTaskPluginReleaseSnapshot.init(jsonValue:))
        let snapshotPairs = snapshots.map { Self.pluginSelectionID(pluginKey: $0.pluginKey, componentKey: $0.componentKey) }
        guard Set(snapshotPairs).count == snapshots.count,
              Set(snapshotPairs) == pluginSelectionIDs else {
            throw NativeLocalAgentPlatformToolError.invalidRunContext
        }
        let pluginReleaseSnapshots = Dictionary(uniqueKeysWithValues: zip(snapshotPairs, snapshots))
        let externalIDs: Set<String>
        if let raw = options["external_mcp_config_ids"] {
            guard case let .array(values) = raw else { throw NativeLocalAgentPlatformToolError.invalidRunContext }
            let ids = try values.map { value -> String in
                guard case let .string(id) = value, !id.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
                    throw NativeLocalAgentPlatformToolError.invalidRunContext
                }
                return id
            }
            externalIDs = Set(ids)
            guard externalIDs.count == ids.count else { throw NativeLocalAgentPlatformToolError.invalidRunContext }
        } else { externalIDs = [] }
        return .init(
            requiresExecution: requiresExecution,
            enabledBuiltinKinds: Set(kinds),
            pluginSelectionIDs: pluginSelectionIDs,
            pluginReleaseSnapshots: pluginReleaseSnapshots,
            ownedPaths: ownedPaths,
            externalMCPConfigIDs: externalIDs
        )
    }

    static func pluginSelectionID(pluginKey: String, componentKey: String) -> String {
        "\(pluginKey)\u{0}\(componentKey)"
    }

    func allows(_ toolName: String) -> Bool {
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
            return capabilityBrokerEnabled
        }
        if NativeLocalAgentPlatformToolCatalog.taskExecutionRemoteToolNames.contains(toolName) {
            return enabledBuiltinKinds.contains("RemoteConnectionController")
        }
        return false
    }
}

struct NativeLocalAgentTaskPluginReleaseSnapshot: Sendable, Equatable {
    let pluginKey: String
    let componentKey: String
    let pluginID: String
    let releaseID: String
    let version: String
    let artifactSHA256: String

    init(
        pluginKey: String,
        componentKey: String,
        pluginID: String,
        releaseID: String,
        version: String,
        artifactSHA256: String
    ) {
        self.pluginKey = pluginKey
        self.componentKey = componentKey
        self.pluginID = pluginID
        self.releaseID = releaseID
        self.version = version
        self.artifactSHA256 = artifactSHA256
    }

    init(jsonValue: LocalAgentJSONValue) throws {
        guard case let .object(values) = jsonValue,
              case let .string(pluginKey)? = values["plugin_key"],
              case let .string(componentKey)? = values["component_key"],
              case let .string(pluginID)? = values["plugin_id"],
              case let .string(releaseID)? = values["release_id"],
              case let .string(version)? = values["version"],
              case let .string(artifactSHA256)? = values["artifact_sha256"],
              !pluginKey.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              !componentKey.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              !pluginID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              !releaseID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              !version.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              !artifactSHA256.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw NativeLocalAgentPlatformToolError.invalidRunContext
        }
        self.init(
            pluginKey: pluginKey,
            componentKey: componentKey,
            pluginID: pluginID,
            releaseID: releaseID,
            version: version,
            artifactSHA256: artifactSHA256
        )
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
        let detail = try await conversations.context(
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
    private let codeReadWorker: any CodeReadWorkerExecuting
    private let pluginTools: NativeLocalAgentPluginToolExecutor
    private let remoteConnections: NativeMCPRemoteConnectionController?

    init(
        host: any LocalAgentHostClientServicing,
        projects: NativeLocalProjectsService,
        connector: NativeLocalConnectorService,
        remoteConnectionProvider: (any NativeRemoteConnectionRuntimeProviding)? = nil,
        writeStore: NativeMCPCodeWriteStore = .init(),
        terminalStore: NativeLocalAgentTerminalStore = .init(),
        codeReadWorker: any CodeReadWorkerExecuting = CodeReadWorkerProcessClient.shared
    ) {
        contextResolver = .init(host: host, projects: projects, connector: connector)
        self.writeStore = writeStore
        self.terminalStore = terminalStore
        self.codeReadWorker = codeReadWorker
        pluginTools = .init(connector: connector, host: host)
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
                        workspaceID: context.workspaceScopeID,
                        sessionID: context.conversationID,
                        runID: invocation.runID
                    ),
                    projectRoot: context.executionRootURL,
                    ownedPaths: context.toolAuthorization.ownedPaths
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
                if ["run_command", "upload_file", "upload_project_file"].contains(upstreamName) {
                    guard invocation.requiresApproval,
                          invocation.approvalStatus == "approved" else {
                        throw NativeLocalAgentPlatformToolError.approvalRequired
                    }
                }
                var boundArguments = nativeArguments
                boundArguments["connection_id"] = .string(remoteConnectionID)
                if upstreamName == "upload_project_file" {
                    result = try await remoteConnections.uploadProjectFile(
                        boundArguments,
                        projectRoot: context.executionRootURL
                    )
                } else {
                    result = try await remoteConnections.call(
                        name: upstreamName,
                        arguments: boundArguments
                    )
                }
            } else {
                guard NativeMCPCodeReadTools.workerToolNames.contains(invocation.toolName) else {
                    throw NativeLocalAgentPlatformToolError.projectToolFailed
                }
                result = try await NativeCodeReadWorkerBridge.execute(
                    name: invocation.toolName,
                    arguments: nativeArguments,
                    projectRoot: context.executionRootURL,
                    requestCWD: nil,
                    defaultToolRoot: nil,
                    client: codeReadWorker
                )
            }
            return .init(native: result)
        } catch let error as NativeLocalAgentPlatformToolError {
            throw error
        } catch let error as NativeMCPCodeWriteError {
            // Staging and pre-commit validation failures have no filesystem side effect.
            // Return the actionable diagnostic to the model so it can re-read and retry
            // instead of escalating an ordinary patch mismatch to Human review.
            if case .commitFailed = error {
                throw NativeLocalAgentPlatformToolError.projectToolFailed
            }
            throw NativeLocalAgentPlatformToolError.projectToolRejected(
                error.errorDescription ?? "The project edit was rejected before commit."
            )
        } catch let error as NativeMCPRemoteConnectionError
            where error.isKnownPreEffectFailure {
            // Argument validation and policy rejection happen before SSH is
            // allowed to mutate the remote host. Preserve an actionable,
            // bounded diagnostic so the model can correct its call instead of
            // incorrectly pausing the whole Run for Human side-effect review.
            throw NativeLocalAgentPlatformToolError.projectToolRejected(
                error.errorDescription ?? "The remote operation was rejected before execution."
            )
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

    func release(runID: String) async {
        _ = await terminalStore.cancel(ownerRunID: runID)
        _ = await writeStore.discard(runID: runID)
        await pluginTools.release(runID: runID)
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
        let root = context.executionRootURL
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
        guard NativeLocalAgentTerminalCommandResolver.purpose(arguments) != nil else {
            throw NativeLocalAgentPlatformToolError.invalidField("purpose")
        }
        let cwd = try NativeLocalAgentExecutionDirectoryResolver.resolve(
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
