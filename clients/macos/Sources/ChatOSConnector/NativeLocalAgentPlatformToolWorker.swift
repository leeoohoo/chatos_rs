import ChatOSCore
import Foundation

public enum NativeLocalAgentPlatformToolCatalog {
    public static let attachmentReadToolName = "local_attachment_read"
    public static let readOnlyToolNames = [attachmentReadToolName]

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
    ]
}

protocol NativeLocalAgentPlatformToolExecuting: Sendable {
    func execute(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) async throws -> LocalAgentJSONValue
}

struct NativeLocalAgentPlatformToolExecutor: NativeLocalAgentPlatformToolExecuting, Sendable {
    private let runtime: NativeLocalAgentRuntimeClient
    private let conversations: NativeLocalAgentConversationClient
    private let attachmentVault: NativeLocalAgentAttachmentVault

    init(host: any LocalAgentHostClientServicing, attachmentRootURL: URL) {
        runtime = .init(host: host)
        conversations = .init(host: host)
        attachmentVault = .init(rootURL: attachmentRootURL)
    }

    func execute(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) async throws -> LocalAgentJSONValue {
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
    private let workerID: String
    private var ownerUserID: String?
    private var generation = UUID()
    private var pollingTask: Task<Void, Never>?
    private var pendingWake = false

    public init(
        host: any LocalAgentHostClientServicing,
        attachmentRootURL: URL,
        workerID: String = "macos-platform-tool-worker"
    ) {
        client = .init(host: host)
        executor = NativeLocalAgentPlatformToolExecutor(
            host: host,
            attachmentRootURL: attachmentRootURL
        )
        self.workerID = workerID
    }

    init(
        client: NativeLocalAgentToolClient,
        executor: any NativeLocalAgentPlatformToolExecuting,
        workerID: String = "macos-platform-tool-worker"
    ) {
        self.client = client
        self.executor = executor
        self.workerID = workerID
    }

    public func configure(ownerUserID: String) {
        resetLocked()
        self.ownerUserID = ownerUserID
        schedulePolling(ownerUserID: ownerUserID, maximumIdleDuration: .zero)
    }

    public func reset() {
        resetLocked()
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

    var errorDescription: String? {
        switch self {
        case .unsupportedTool: "The requested local platform tool is unavailable."
        case .invalidArguments: "The local platform tool arguments are invalid."
        case let .invalidField(field): "The local platform tool field is invalid: \(field)."
        case .invalidRunContext: "The local platform tool Run context is invalid."
        case .attachmentNotAuthorized: "The attachment is not authorized for this conversation."
        }
    }
}
