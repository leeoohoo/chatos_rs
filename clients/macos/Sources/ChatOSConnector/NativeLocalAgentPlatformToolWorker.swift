import ChatOSCore
import Foundation

protocol NativeLocalAgentPlatformToolExecuting: Sendable {
  func execute(
    ownerUserID: String,
    invocation: LocalAgentToolInvocationRecord
  ) async throws -> LocalAgentJSONValue
  func configureExternalMCPs(
    _ configs: [NativeLocalAgentExternalMCPConfig]
  ) async throws
  func reset() async
}

extension NativeLocalAgentPlatformToolExecuting {
  func configureExternalMCPs(
    _: [NativeLocalAgentExternalMCPConfig]
  ) async throws {}
  func reset() async {}
}

struct NativeLocalAgentPlatformToolExecutor: NativeLocalAgentPlatformToolExecuting, Sendable {
  private let runtime: NativeLocalAgentRuntimeClient
  private let conversations: NativeLocalAgentConversationClient
  private let attachmentVault: NativeLocalAgentAttachmentVault
  private let projectTools: (any NativeLocalAgentProjectToolExecuting)?
  private let externalMCPs: NativeLocalAgentExternalMCPExecutor

  init(
    host: any LocalAgentHostClientServicing,
    attachmentRootURL: URL,
    projectTools: (any NativeLocalAgentProjectToolExecuting)? = nil
  ) {
    runtime = .init(host: host)
    conversations = .init(host: host)
    attachmentVault = .init(rootURL: attachmentRootURL)
    self.projectTools = projectTools
    externalMCPs = .init(host: host)
  }

  func execute(
    ownerUserID: String,
    invocation: LocalAgentToolInvocationRecord
  ) async throws -> LocalAgentJSONValue {
    if await externalMCPs.contains(invocation.toolName) {
      return try await externalMCPs.execute(
        ownerUserID: ownerUserID,
        invocation: invocation
      )
    }
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
      case .object(let input) = run.input,
      case .string(let conversationID)? =
        input["source_conversation_id"] ?? input["conversation_id"]
    else {
      throw NativeLocalAgentPlatformToolError.invalidRunContext
    }
    let conversation = try await conversations.get(
      ownerUserID: ownerUserID,
      conversationID: conversationID
    )
    guard
      let attachment = conversation.attachments.first(where: {
        $0.authorizedLocalRef == authorizedLocalRef
      })
    else {
      throw NativeLocalAgentPlatformToolError.attachmentNotAuthorized
    }
    do {
      return try await attachmentVault.resolve(
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
    await externalMCPs.reset()
    await projectTools?.reset()
  }

  func configureExternalMCPs(
    _ configs: [NativeLocalAgentExternalMCPConfig]
  ) async throws {
    try await externalMCPs.configure(configs)
  }

  private static func object(
    _ value: LocalAgentJSONValue
  ) throws -> [String: LocalAgentJSONValue] {
    guard case .object(let object) = value else {
      throw NativeLocalAgentPlatformToolError.invalidArguments
    }
    return object
  }

  private static func string(
    _ value: LocalAgentJSONValue?,
    field: String
  ) throws -> String {
    guard case .string(let result)? = value, !result.isEmpty else {
      throw NativeLocalAgentPlatformToolError.invalidField(field)
    }
    return result
  }

  private static func integer(
    _ value: LocalAgentJSONValue,
    field: String
  ) throws -> Int {
    guard case .number(let number) = value,
      number.isFinite,
      number.rounded() == number,
      let result = Int(exactly: number)
    else {
      throw NativeLocalAgentPlatformToolError.invalidField(field)
    }
    return result
  }
}

enum NativeLocalAgentPlatformToolPollingPolicy {
  static let activityWindow = Duration.seconds(15)
  static let eventMonitoringWindow = Duration.seconds(300)

  static func shouldWake(forEventTypes eventTypes: [String]) -> Bool {
    eventTypes.contains { eventType in
      eventType == "tool_batch_requested"
        || eventType == "tool_invocation_approved"
        || eventType == "tool_claim_expired_requeued"
    }
  }
}

public actor NativeLocalAgentPlatformToolWorker {
  private enum ClaimExecutionResult: Sendable {
    case outcome(LocalAgentToolOutcome)
    case leaseLost
  }

  private let client: NativeLocalAgentToolClient
  private let executor: any NativeLocalAgentPlatformToolExecuting
  private let approvalHandler: (any NativeLocalAgentToolApprovalHandling)?
  private let eventHub: NativeLocalAgentEventHub?
  private let workerID: String
  private let claimLeaseDurationMilliseconds: UInt64
  private let claimHeartbeatInterval: Duration
  private let activityWindow: Duration
  private var ownerUserID: String?
  private var generation = UUID()
  private var pollingTask: Task<Void, Never>?
  private var eventWakeTask: Task<Void, Never>?
  private var eventWakeTimeoutTask: Task<Void, Never>?
  private var pendingWake = false

  public init(
    host: any LocalAgentHostClientServicing,
    attachmentRootURL: URL,
    projects: NativeLocalProjectsService,
    connector: NativeLocalConnectorService,
    remoteConnectionProvider: (any NativeRemoteConnectionRuntimeProviding)? = nil,
    eventHub: NativeLocalAgentEventHub? = nil,
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
        remoteConnectionProvider: remoteConnectionProvider,
        writeStore: writeStore,
        terminalStore: terminalStore
      )
    )
    approvalHandler = NativeLocalAgentToolApprovalHandler(
      host: host,
      projects: projects,
      connector: connector
    )
    self.eventHub = eventHub
    self.workerID = workerID
    claimLeaseDurationMilliseconds = 30_000
    claimHeartbeatInterval = .seconds(10)
    activityWindow = NativeLocalAgentPlatformToolPollingPolicy.activityWindow
  }

  init(
    client: NativeLocalAgentToolClient,
    executor: any NativeLocalAgentPlatformToolExecuting,
    approvalHandler: (any NativeLocalAgentToolApprovalHandling)? = nil,
    eventHub: NativeLocalAgentEventHub? = nil,
    workerID: String = "macos-platform-tool-worker",
    claimLeaseDurationMilliseconds: UInt64 = 30_000,
    claimHeartbeatInterval: Duration = .seconds(10),
    activityWindow: Duration = NativeLocalAgentPlatformToolPollingPolicy.activityWindow
  ) {
    self.client = client
    self.executor = executor
    self.approvalHandler = approvalHandler
    self.eventHub = eventHub
    self.workerID = workerID
    self.claimLeaseDurationMilliseconds = claimLeaseDurationMilliseconds
    self.claimHeartbeatInterval = claimHeartbeatInterval
    self.activityWindow = activityWindow
  }

  public func configure(ownerUserID: String) async {
    try? await configure(ownerUserID: ownerUserID, externalMCPConfigs: [])
  }

  public func configure(
    ownerUserID: String,
    externalMCPConfigs: [NativeLocalAgentExternalMCPConfig]
  ) async throws {
    resetLocked()
    await executor.reset()
    try await executor.configureExternalMCPs(externalMCPConfigs)
    self.ownerUserID = ownerUserID
    schedulePolling(ownerUserID: ownerUserID, maximumIdleDuration: .zero)
  }

  public func reset() async {
    resetLocked()
    await executor.reset()
    ownerUserID = nil
  }

  /// Starts one short compatibility window. Durable tool-request and approval
  /// events wake the worker again, so model think time no longer requires a
  /// five-minute claim loop.
  public func wake() {
    guard let ownerUserID else { return }
    startEventMonitoring(ownerUserID: ownerUserID)
    guard pollingTask == nil else {
      pendingWake = true
      return
    }
    schedulePolling(ownerUserID: ownerUserID, maximumIdleDuration: activityWindow)
  }

  private func startEventMonitoring(ownerUserID: String) {
    guard let eventHub else { return }
    let expectedGeneration = generation
    if eventWakeTask == nil {
      eventWakeTask = Task { [weak self, eventHub] in
        await eventHub.configure(ownerUserID: ownerUserID)
        let updates = await eventHub.updates()
        for await update in updates {
          guard !Task.isCancelled,
            update.ownerUserID == ownerUserID
          else { continue }
          guard case .events(let events) = update.kind,
            NativeLocalAgentPlatformToolPollingPolicy.shouldWake(
              forEventTypes: events.map(\.eventType)
            )
          else { continue }
          await self?.wakeFromEvent(
            ownerUserID: ownerUserID,
            generation: expectedGeneration
          )
        }
      }
    }
    eventWakeTimeoutTask?.cancel()
    eventWakeTimeoutTask = Task { [weak self] in
      do {
        try await Task.sleep(
          for: NativeLocalAgentPlatformToolPollingPolicy.eventMonitoringWindow
        )
      } catch {
        return
      }
      guard !Task.isCancelled else { return }
      await self?.stopEventMonitoring(
        ownerUserID: ownerUserID,
        generation: expectedGeneration
      )
    }
  }

  private func wakeFromEvent(ownerUserID: String, generation: UUID) {
    guard self.ownerUserID == ownerUserID,
      self.generation == generation
    else { return }
    wake()
  }

  private func stopEventMonitoring(ownerUserID: String, generation: UUID) {
    guard self.ownerUserID == ownerUserID,
      self.generation == generation
    else { return }
    eventWakeTask?.cancel()
    eventWakeTask = nil
    eventWakeTimeoutTask?.cancel()
    eventWakeTimeoutTask = nil
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
            maximumIdleDuration: activityWindow
          )
        }
      }
    }
    while !Task.isCancelled,
      generation == expectedGeneration,
      self.ownerUserID == ownerUserID
    {
      do {
        if try await approvalHandler?.resolveNextPending(
          ownerUserID: ownerUserID
        ) == true {
          idleDelay = .milliseconds(250)
          continue
        }
        if let claim = try await client.claimNext(
          ownerUserID: ownerUserID,
          workerID: workerID,
          leaseDurationMilliseconds: claimLeaseDurationMilliseconds
        ) {
          guard
            let outcome = await executeWhileRenewing(
              ownerUserID: ownerUserID,
              claim: claim
            )
          else {
            return
          }
          guard !Task.isCancelled,
            generation == expectedGeneration,
            self.ownerUserID == ownerUserID
          else {
            return
          }
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

  private func executeWhileRenewing(
    ownerUserID: String,
    claim: LocalAgentToolClaim
  ) async -> LocalAgentToolOutcome? {
    let client = self.client
    let executor = self.executor
    let leaseDurationMilliseconds = claimLeaseDurationMilliseconds
    let heartbeatInterval = claimHeartbeatInterval
    return await withTaskGroup(
      of: ClaimExecutionResult.self,
      returning: LocalAgentToolOutcome?.self
    ) { group in
      group.addTask {
        .outcome(
          await Self.execute(
            executor: executor,
            ownerUserID: ownerUserID,
            claim: claim
          ))
      }
      group.addTask {
        do {
          while !Task.isCancelled {
            try await Task.sleep(for: heartbeatInterval)
            try Task.checkCancellation()
            guard
              try await client.renew(
                ownerUserID: ownerUserID,
                claim: claim,
                leaseDurationMilliseconds: leaseDurationMilliseconds
              )
            else {
              return .leaseLost
            }
          }
        } catch is CancellationError {
          return .leaseLost
        } catch {
          return .leaseLost
        }
        return .leaseLost
      }

      guard let first = await group.next() else {
        group.cancelAll()
        return nil
      }
      group.cancelAll()
      switch first {
      case .outcome(let outcome): return outcome
      case .leaseLost: return nil
      }
    }
  }

  private static func execute(
    executor: any NativeLocalAgentPlatformToolExecuting,
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
    eventWakeTask?.cancel()
    eventWakeTask = nil
    eventWakeTimeoutTask?.cancel()
    eventWakeTimeoutTask = nil
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
  case capabilityNotSelected

  var errorDescription: String? {
    switch self {
    case .unsupportedTool: "The requested local platform tool is unavailable."
    case .invalidArguments: "The local platform tool arguments are invalid."
    case .invalidField(let field): "The local platform tool field is invalid: \(field)."
    case .invalidRunContext: "The local platform tool Run context is invalid."
    case .attachmentNotAuthorized: "The attachment is not authorized for this conversation."
    case .projectUnavailable: "The local project is unavailable for this conversation."
    case .projectToolFailed: "The local project tool arguments or state are invalid."
    case .approvalRequired: "The local project change requires approval before execution."
    case .capabilityNotSelected: "This Task did not select the requested local capability."
    }
  }
}
