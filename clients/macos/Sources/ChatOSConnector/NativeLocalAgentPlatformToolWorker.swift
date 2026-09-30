import ChatOSCore
import Foundation

public enum NativeLocalAgentPlatformToolCatalog {
    public static let attachmentReadToolName = "local_attachment_read"
    public static let createTaskToolName = "create_task"
    public static let createTasksToolName = "create_tasks_with_prerequisites"
    private static let projectReadOnlyToolNames = [
        "read_file_raw", "read_file_range", "list_dir", "search_text", "read_file",
        "search_files",
    ]
    private static let terminalReadOnlyToolNames = [
        "process_poll", "process_log", "process_wait",
    ]
    private static let requirementSurveyReadOnlyToolNames =
        NativeMCPRequirementSurveyTools.readToolNames.sorted()
    private static let pluginReadOnlyToolNames =
        NativeAgentCapabilityBrokerToolCatalog.readOnlyToolNames.sorted()
    static let taskExecutionTerminalToolNames: Set<String> = [
        "execute_command", "process_poll", "process_log", "process_wait", "process_write",
        "process_kill",
    ]
    static let taskExecutionRequirementSurveyToolNames =
        NativeMCPRequirementSurveyTools.readToolNames
            .union(NativeMCPRequirementSurveyTools.writeToolNames)
    public static let readOnlyToolNames = [attachmentReadToolName]
        + projectReadOnlyToolNames + terminalReadOnlyToolNames
        + requirementSurveyReadOnlyToolNames + pluginReadOnlyToolNames
    public static let approvalExemptToolNames = [
        "open_edit_session", "stage_edit_batch", "abort_edit_session",
        NativeAgentCapabilityBrokerToolCatalog.invokeToolName,
    ]

    public static let capabilityTools: [LocalAgentJSONValue] = [
        .object([
            "type": .string("function"),
            "name": .string(attachmentReadToolName),
            "description": .string(
                "Read a bounded segment of a local conversation attachment using its opaque authorized_local_ref. The client verifies account, conversation, byte size, and SHA-256 before returning content; local filesystem paths are never exposed."
            ),
            "parameters": .object([
                "type": .string("object"),
                "properties": .object([
                    "authorized_local_ref": .object([
                        "type": .string("string"),
                        "minLength": .number(1),
                        "maxLength": .number(160),
                    ]),
                    "offset": .object([
                        "type": .string("integer"),
                        "minimum": .number(0),
                        "default": .number(0),
                    ]),
                    "limit": .object([
                        "type": .string("integer"),
                        "minimum": .number(1),
                        "maximum": .number(Double(NativeLocalAgentAttachmentVault.maximumReadBytes)),
                        "default": .number(16_384),
                    ]),
                ]),
                "required": .array([.string("authorized_local_ref")]),
                "additionalProperties": .bool(false),
            ]),
        ]),
        .object([
            "type": .string("function"),
            "name": .string(createTaskToolName),
            "description": .string(
                "Create one durable local task derived from the current conversation. Use it only when the user asks for work that should continue as a tracked task. The Rust Local Agent Host persists and schedules the task locally."
            ),
            "parameters": .object([
                "type": .string("object"),
                "properties": .object([
                    "title": .object(["type": .string("string"), "minLength": .number(1)]),
                    "objective": .object(["type": .string("string"), "minLength": .number(1)]),
                    "description": .object(["type": .string("string")]),
                    "input_payload": .object(["type": .string("object")]),
                ]),
                "required": .array([.string("title"), .string("objective")]),
                "additionalProperties": .bool(false),
            ]),
        ]),
        .object([
            "type": .string("function"),
            "name": .string(createTasksToolName),
            "description": .string(
                "Create a durable local task graph. Each task uses a unique client_ref; prerequisite_refs may only reference tasks in this same call. The Rust Local Agent Host validates, persists, and schedules the DAG locally."
            ),
            "parameters": .object([
                "type": .string("object"),
                "properties": .object([
                    "tasks": .object([
                        "type": .string("array"),
                        "minItems": .number(1),
                        "maxItems": .number(50),
                        "items": .object([
                            "type": .string("object"),
                            "properties": .object([
                                "client_ref": .object(["type": .string("string"), "minLength": .number(1)]),
                                "title": .object(["type": .string("string"), "minLength": .number(1)]),
                                "objective": .object(["type": .string("string"), "minLength": .number(1)]),
                                "description": .object(["type": .string("string")]),
                                "input_payload": .object(["type": .string("object")]),
                                "prerequisite_refs": .object([
                                    "type": .string("array"),
                                    "items": .object(["type": .string("string"), "minLength": .number(1)]),
                                    "uniqueItems": .bool(true),
                                ]),
                            ]),
                            "required": .array([
                                .string("client_ref"), .string("title"), .string("objective"),
                            ]),
                            "additionalProperties": .bool(false),
                        ]),
                    ]),
                ]),
                "required": .array([.string("tasks")]),
                "additionalProperties": .bool(false),
            ]),
        ]),
    ]

    public static let taskExecutionCapabilityTools: [LocalAgentJSONValue] =
        NativeMCPCodeReadTools.toolDefinitions.compactMap { value in
            guard case let .object(tool) = value,
                  case let .string(name)? = tool["name"],
                  projectReadOnlyToolNames.contains(name) else { return nil }
            return capabilityTool(value)
        }
        + NativeMCPCodeWriteStore.toolDefinitions.map(capabilityTool)
        + NativeMCPTerminalStore.toolDefinitions.compactMap { value in
            guard case let .object(tool) = value,
                  case let .string(name)? = tool["name"],
                  taskExecutionTerminalToolNames.contains(name) else { return nil }
            return capabilityTool(value)
        }
        + NativeMCPRequirementSurveyTools.readToolDefinitions.map(capabilityTool)
        + NativeMCPRequirementSurveyTools.writeToolDefinitions.map(capabilityTool)
        + NativeAgentCapabilityBrokerToolCatalog.localAgentCapabilityTools

    static var taskExecutionToolNames: Set<String> {
        Set(taskExecutionCapabilityTools.compactMap { value in
            guard case let .object(tool) = value,
                  case let .string(name)? = tool["name"] else { return nil }
            return name
        })
    }

    private static func capabilityTool(_ value: NativeJSONValue) -> LocalAgentJSONValue {
        guard case let .object(tool) = value,
              case let .string(name)? = tool["name"],
              case let .object(schema)? = tool["inputSchema"] else {
            preconditionFailure("Native project-read tool definition is invalid")
        }
        let description: String
        if case let .string(value)? = tool["description"] { description = value }
        else { description = "" }
        return .object([
            "type": .string("function"),
            "name": .string(name),
            "description": .string(description),
            "parameters": .object(schema.mapValues(LocalAgentJSONValue.init(native:))),
        ])
    }
}

protocol NativeLocalAgentPlatformToolExecuting: Sendable {
    func execute(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) async throws -> LocalAgentJSONValue
    func reset() async
}

extension NativeLocalAgentPlatformToolExecuting {
    func reset() async {}
}

struct NativeLocalAgentPlatformToolExecutor: NativeLocalAgentPlatformToolExecuting, Sendable {
    private let runtime: NativeLocalAgentRuntimeClient
    private let conversations: NativeLocalAgentConversationClient
    private let attachmentVault: NativeLocalAgentAttachmentVault
    private let projectTools: (any NativeLocalAgentProjectToolExecuting)?

    init(
        host: any LocalAgentHostClientServicing,
        attachmentRootURL: URL,
        projectTools: (any NativeLocalAgentProjectToolExecuting)? = nil
    ) {
        runtime = .init(host: host)
        conversations = .init(host: host)
        attachmentVault = .init(rootURL: attachmentRootURL)
        self.projectTools = projectTools
    }

    func execute(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) async throws -> LocalAgentJSONValue {
        if NativeLocalAgentPlatformToolCatalog.taskExecutionToolNames.contains(invocation.toolName) {
            guard let projectTools else {
                throw NativeLocalAgentPlatformToolError.projectUnavailable
            }
            return try await projectTools.execute(
                ownerUserID: ownerUserID,
                invocation: invocation
            )
        }
        guard invocation.toolName == NativeLocalAgentPlatformToolCatalog.attachmentReadToolName else {
            throw NativeLocalAgentPlatformToolError.unsupportedTool
        }
        let arguments = try Self.object(invocation.arguments)
        let authorizedLocalRef = try Self.string(
            arguments["authorized_local_ref"],
            field: "authorized_local_ref"
        )
        let offset = try Self.integer(arguments["offset"] ?? .number(0), field: "offset")
        let limit = try Self.integer(
            arguments["limit"] ?? .number(16_384),
            field: "limit"
        )
        guard offset >= 0, limit > 0, limit <= NativeLocalAgentAttachmentVault.maximumReadBytes else {
            throw NativeLocalAgentPlatformToolError.invalidArguments
        }
        let run = try await runtime.run(ownerUserID: ownerUserID, runID: invocation.runID)
        guard run.ownerUserID == ownerUserID,
              case let .object(input) = run.input,
              case let .string(conversationID)? = input["conversation_id"] else {
            throw NativeLocalAgentPlatformToolError.invalidRunContext
        }
        let conversation = try await conversations.get(
            ownerUserID: ownerUserID,
            conversationID: conversationID
        )
        guard let attachment = conversation.attachments.first(where: {
            $0.authorizedLocalRef == authorizedLocalRef
        }) else {
            throw NativeLocalAgentPlatformToolError.attachmentNotAuthorized
        }
        do {
            return try attachmentVault.resolve(
                attachment,
                ownerUserID: ownerUserID,
                conversationID: conversationID,
                offset: UInt64(offset),
                limit: limit
            ).jsonValue
        } catch let error as NativeLocalAgentAttachmentVaultError {
            throw error
        } catch {
            // Filesystem errors can contain a real local path. Collapse them
            // before the outcome crosses IPC and becomes model-visible.
            throw NativeLocalAgentAttachmentVaultError.invalidAttachment
        }
    }

    func reset() async {
        await projectTools?.reset()
    }

    private static func object(
        _ value: LocalAgentJSONValue
    ) throws -> [String: LocalAgentJSONValue] {
        guard case let .object(object) = value else {
            throw NativeLocalAgentPlatformToolError.invalidArguments
        }
        return object
    }

    private static func string(
        _ value: LocalAgentJSONValue?,
        field: String
    ) throws -> String {
        guard case let .string(result)? = value, !result.isEmpty else {
            throw NativeLocalAgentPlatformToolError.invalidField(field)
        }
        return result
    }

    private static func integer(
        _ value: LocalAgentJSONValue,
        field: String
    ) throws -> Int {
        guard case let .number(number) = value,
              number.isFinite,
              number.rounded() == number,
              let result = Int(exactly: number) else {
            throw NativeLocalAgentPlatformToolError.invalidField(field)
        }
        return result
    }
}

public actor NativeLocalAgentPlatformToolWorker {
    private let client: NativeLocalAgentToolClient
    private let executor: any NativeLocalAgentPlatformToolExecuting
    private let approvalHandler: (any NativeLocalAgentToolApprovalHandling)?
    private let workerID: String
    private var ownerUserID: String?
    private var generation = UUID()
    private var pollingTask: Task<Void, Never>?
    private var pendingWake = false

    public init(
        host: any LocalAgentHostClientServicing,
        attachmentRootURL: URL,
        projects: NativeLocalProjectsService,
        connector: NativeLocalConnectorService,
        agentGroupChats: NativeAgentGroupChatService,
        workerID: String = "macos-platform-tool-worker"
    ) {
        client = .init(host: host)
        let writeStore = NativeMCPCodeWriteStore()
        let terminalStore = NativeLocalAgentTerminalStore()
        executor = NativeLocalAgentPlatformToolExecutor(
            host: host,
            attachmentRootURL: attachmentRootURL,
            projectTools: NativeLocalAgentProjectToolExecutor(
                host: host,
                projects: projects,
                connector: connector,
                writeStore: writeStore,
                terminalStore: terminalStore,
                agentGroupChats: agentGroupChats
            )
        )
        approvalHandler = NativeLocalAgentToolApprovalHandler(
            host: host,
            projects: projects,
            connector: connector
        )
        self.workerID = workerID
    }

    init(
        client: NativeLocalAgentToolClient,
        executor: any NativeLocalAgentPlatformToolExecuting,
        approvalHandler: (any NativeLocalAgentToolApprovalHandling)? = nil,
        workerID: String = "macos-platform-tool-worker"
    ) {
        self.client = client
        self.executor = executor
        self.approvalHandler = approvalHandler
        self.workerID = workerID
    }

    public func configure(ownerUserID: String) async {
        resetLocked()
        await executor.reset()
        self.ownerUserID = ownerUserID
        schedulePolling(ownerUserID: ownerUserID, maximumIdleDuration: .zero)
    }

    public func reset() async {
        resetLocked()
        await executor.reset()
        ownerUserID = nil
    }

    /// Starts one bounded activity window. It backs off while the model is still
    /// running and stops after five idle minutes; it is not a permanent poller.
    public func wake() {
        guard let ownerUserID else { return }
        guard pollingTask == nil else {
            pendingWake = true
            return
        }
        schedulePolling(ownerUserID: ownerUserID, maximumIdleDuration: .seconds(300))
    }

    private func schedulePolling(ownerUserID: String, maximumIdleDuration: Duration) {
        let token = generation
        pollingTask = Task { [weak self] in
            await self?.poll(
                ownerUserID: ownerUserID,
                generation: token,
                maximumIdleDuration: maximumIdleDuration
            )
        }
    }

    private func poll(
        ownerUserID: String,
        generation expectedGeneration: UUID,
        maximumIdleDuration: Duration
    ) async {
        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: maximumIdleDuration)
        var idleDelay = Duration.milliseconds(250)
        defer {
            if generation == expectedGeneration {
                pollingTask = nil
                if pendingWake, let ownerUserID = self.ownerUserID {
                    pendingWake = false
                    schedulePolling(
                        ownerUserID: ownerUserID,
                        maximumIdleDuration: .seconds(300)
                    )
                }
            }
        }
        while !Task.isCancelled,
              generation == expectedGeneration,
              self.ownerUserID == ownerUserID {
            do {
                if try await approvalHandler?.resolveNextPending(
                    ownerUserID: ownerUserID
                ) == true {
                    idleDelay = .milliseconds(250)
                    continue
                }
                if let claim = try await client.claimNext(
                    ownerUserID: ownerUserID,
                    workerID: workerID
                ) {
                    let outcome = await execute(ownerUserID: ownerUserID, claim: claim)
                    _ = try await client.commit(
                        ownerUserID: ownerUserID,
                        claim: claim,
                        outcome: outcome
                    )
                    idleDelay = .milliseconds(250)
                    continue
                }
            } catch is CancellationError {
                return
            } catch {
                if maximumIdleDuration == .zero { return }
            }
            guard maximumIdleDuration != .zero, clock.now < deadline else { return }
            do {
                try await Task.sleep(for: idleDelay)
            } catch {
                return
            }
            idleDelay = min(idleDelay * 2, .seconds(2))
        }
    }

    private func execute(
        ownerUserID: String,
        claim: LocalAgentToolClaim
    ) async -> LocalAgentToolOutcome {
        do {
            let output = try await executor.execute(
                ownerUserID: ownerUserID,
                invocation: claim.invocation
            )
            return .succeeded(output)
        } catch {
            let summary = Self.safeErrorSummary(error)
            let detail: LocalAgentJSONValue = .object([
                "tool_name": .string(claim.invocation.toolName),
                "phase": .string("native_platform_execution"),
            ])
            return claim.invocation.sideEffecting
                ? .needsReview(reason: summary, detail: detail)
                : .failed(error: summary, detail: detail)
        }
    }

    private func resetLocked() {
        generation = UUID()
        pollingTask?.cancel()
        pollingTask = nil
        pendingWake = false
    }

    private static func safeErrorSummary(_ error: Error) -> String {
        let raw: String
        switch error {
        case let error as NativeLocalAgentPlatformToolError:
            raw = error.errorDescription ?? "Local platform tool failed."
        case let error as NativeLocalAgentAttachmentVaultError:
            raw = error.errorDescription ?? "Local attachment access failed."
        default:
            // Do not forward arbitrary native error descriptions. They can
            // contain filesystem paths, process arguments, or credentials.
            raw = "Local platform tool failed."
        }
        let lowered = raw.lowercased()
        let sensitive = ["authorization", "api_key", "apikey", "access_token", "password"]
        guard !sensitive.contains(where: lowered.contains) else {
            return "Local platform tool failed; sensitive details were hidden."
        }
        return String(raw.prefix(1_000))
    }
}

enum NativeLocalAgentPlatformToolError: LocalizedError, Equatable {
    case unsupportedTool
    case invalidArguments
    case invalidField(String)
    case invalidRunContext
    case attachmentNotAuthorized
    case projectUnavailable
    case projectToolFailed
    case approvalRequired

    var errorDescription: String? {
        switch self {
        case .unsupportedTool: "The requested local platform tool is unavailable."
        case .invalidArguments: "The local platform tool arguments are invalid."
        case let .invalidField(field): "The local platform tool field is invalid: \(field)."
        case .invalidRunContext: "The local platform tool Run context is invalid."
        case .attachmentNotAuthorized: "The attachment is not authorized for this conversation."
        case .projectUnavailable: "The local project is unavailable for this conversation."
        case .projectToolFailed: "The local project tool arguments or state are invalid."
        case .approvalRequired: "The local project change requires approval before execution."
        }
    }
}
