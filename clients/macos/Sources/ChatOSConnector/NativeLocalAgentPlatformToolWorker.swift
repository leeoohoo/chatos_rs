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
  func release(runID: String) async
  func reset() async
}

extension NativeLocalAgentPlatformToolExecuting {
  func configureExternalMCPs(
    _: [NativeLocalAgentExternalMCPConfig]
  ) async throws {}
  func release(runID _: String) async {}
  func reset() async {}
}

struct NativeLocalAgentPlatformToolExecutor: NativeLocalAgentPlatformToolExecuting, Sendable {
  private let runtime: NativeLocalAgentRuntimeClient
  private let conversations: NativeLocalAgentConversationClient
  private let attachmentVault: NativeLocalAgentAttachmentVault
  private let attachmentWorkspaceResolver: (any NativeLocalAgentAttachmentWorkspaceResolving)?
  private let projectTools: (any NativeLocalAgentProjectToolExecuting)?
  private let externalMCPs: NativeLocalAgentExternalMCPExecutor

  init(
    host: any LocalAgentHostClientServicing,
    attachmentRootURL: URL,
    projectTools: (any NativeLocalAgentProjectToolExecuting)? = nil,
    attachmentWorkspaceResolver: (any NativeLocalAgentAttachmentWorkspaceResolving)? = nil
  ) {
    runtime = .init(host: host)
    conversations = .init(host: host)
    attachmentVault = .init(rootURL: attachmentRootURL)
    self.projectTools = projectTools
    self.attachmentWorkspaceResolver = attachmentWorkspaceResolver
    externalMCPs = .init(host: host)
  }

  func execute(
    ownerUserID: String,
    invocation: LocalAgentToolInvocationRecord
  ) async throws -> LocalAgentJSONValue {
    if invocation.toolName.hasPrefix("external_mcp__") {
      return try await externalMCPs.execute(
        ownerUserID: ownerUserID,
        invocation: invocation
      )
    }
    // The execution catalog also advertises attachment reads. They are scoped
    // by conversation authorization, not by project/builtin capability grants.
    if invocation.toolName != NativeLocalAgentPlatformToolCatalog.attachmentReadToolName,
      NativeLocalAgentPlatformToolCatalog.taskExecutionToolNames.contains(invocation.toolName)
    {
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
    let delivery = try Self.string(
      arguments["delivery"] ?? .string(run.profileKey == "main_chat" ? "model" : "workspace"),
      field: "delivery"
    )
    guard ["workspace", "content", "model"].contains(delivery),
      !(run.profileKey == "main_chat" && delivery == "workspace") else {
      throw NativeLocalAgentPlatformToolError.invalidArguments
    }
    let conversation = try await conversations.get(
      ownerUserID: ownerUserID,
      conversationID: conversationID
    )
    guard conversation.conversation.ownerUserID == ownerUserID,
      conversation.conversation.conversationID == conversationID
    else {
      throw NativeLocalAgentPlatformToolError.invalidRunContext
    }
    guard
      let attachment = conversation.attachments.first(where: {
        $0.authorizedLocalRef == authorizedLocalRef && $0.conversationID == conversationID
      })
    else {
      throw NativeLocalAgentPlatformToolError.attachmentNotAuthorized
    }
    let workspaceRoot: URL?
    if delivery == "workspace" {
      guard let attachmentWorkspaceResolver else {
        throw NativeLocalAgentAttachmentWorkspaceError.unavailable
      }
      workspaceRoot = try await attachmentWorkspaceResolver.attachmentWorkspaceRoot(
        ownerUserID: ownerUserID, runID: invocation.runID
      )
    } else {
      workspaceRoot = nil
    }
    do {
      if delivery == "model" {
        let data = try await attachmentVault.verifiedData(
          attachment, ownerUserID: ownerUserID, conversationID: conversationID
        )
        return try await NativeLocalAgentAttachmentModelInput.result(data, record: attachment)
      }
      if let workspaceRoot {
        let data = try await attachmentVault.verifiedData(
          attachment, ownerUserID: ownerUserID, conversationID: conversationID
        )
        let imported = try await NativeLocalAgentAttachmentWorkspace.materialize(
          data, record: attachment, ownerUserID: ownerUserID,
          conversationID: conversationID, rootURL: workspaceRoot
        ).jsonValue
        guard attachment.mediaType.lowercased().hasPrefix("image/") else { return imported }
        return try await NativeLocalAgentAttachmentModelInput.result(
          data, record: attachment, imported: imported
        )
      }
      return try await attachmentVault.resolve(
        attachment,
        ownerUserID: ownerUserID,
        conversationID: conversationID,
        offset: UInt64(offset),
        limit: min(limit, NativeLocalAgentAttachmentVault.maximumInlineReadBytes)
      ).jsonValue
    } catch let error as NativeLocalAgentAttachmentVaultError {
      throw error
    } catch let error as NativeLocalAgentAttachmentWorkspaceError {
      throw error
    } catch let error as NativeLocalAgentAttachmentModelInputError {
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

  func release(runID: String) async {
    await projectTools?.release(runID: runID)
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

  static func shouldWake(forEventTypes eventTypes: [String]) -> Bool {
    eventTypes.contains { eventType in
      eventType == "tool_batch_requested"
        || eventType == "tool_invocation_approved"
        || eventType == "tool_claim_expired_requeued"
    }
  }

  static func shouldReleaseRunResources(forEventType eventType: String) -> Bool {
    eventType == "run_succeeded"
      || eventType == "run_failed"
      || eventType == "run_cancelled"
  }
}

public actor NativeLocalAgentPlatformToolWorker {
  private enum ClaimExecutionResult: Sendable {
    case outcome(LocalAgentToolOutcome)
    case leaseLost
  }

  private enum PollGroupResult: Sendable {
    case claimFinished(Bool)
    case refill
  }

  private let client: NativeLocalAgentToolClient
  private let executor: any NativeLocalAgentPlatformToolExecuting
  private let approvalHandler: (any NativeLocalAgentToolApprovalHandling)?
  private let eventHub: NativeLocalAgentEventHub?
  private let workerID: String
  private let claimLeaseDurationMilliseconds: UInt64
  private let claimHeartbeatInterval: Duration
  private let activityWindow: Duration
  private let maximumConcurrentClaims: Int
  private var ownerUserID: String?
  private var generation = UUID()
  private var pollingTask: Task<Void, Never>?
  private var eventWakeTask: Task<Void, Never>?
  private var pendingWake = false

  public init(
    host: any LocalAgentHostClientServicing,
    attachmentRootURL: URL,
    projects: NativeLocalProjectsService,
    connector: NativeLocalConnectorService,
    remoteConnectionProvider: (any NativeRemoteConnectionRuntimeProviding)? = nil,
    eventHub: NativeLocalAgentEventHub? = nil,
    workerID: String = "macos-platform-tool-worker",
    maximumConcurrentClaims: Int = NativeLocalAgentConcurrencyPolicy.adaptiveDefault
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
      ),
      attachmentWorkspaceResolver: NativeLocalAgentProjectContextResolver(
        host: host, projects: projects, connector: connector
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
    self.maximumConcurrentClaims = NativeLocalAgentConcurrencyPolicy.validated(
      maximumConcurrentClaims
    ) ?? NativeLocalAgentConcurrencyPolicy.adaptiveDefault
  }

  init(
    client: NativeLocalAgentToolClient,
    executor: any NativeLocalAgentPlatformToolExecuting,
    approvalHandler: (any NativeLocalAgentToolApprovalHandling)? = nil,
    eventHub: NativeLocalAgentEventHub? = nil,
    workerID: String = "macos-platform-tool-worker",
    claimLeaseDurationMilliseconds: UInt64 = 30_000,
    claimHeartbeatInterval: Duration = .seconds(10),
    activityWindow: Duration = NativeLocalAgentPlatformToolPollingPolicy.activityWindow,
    maximumConcurrentClaims: Int = NativeLocalAgentConcurrencyPolicy.adaptiveDefault
  ) {
    self.client = client
    self.executor = executor
    self.approvalHandler = approvalHandler
    self.eventHub = eventHub
    self.workerID = workerID
    self.claimLeaseDurationMilliseconds = claimLeaseDurationMilliseconds
    self.claimHeartbeatInterval = claimHeartbeatInterval
    self.activityWindow = activityWindow
    self.maximumConcurrentClaims = NativeLocalAgentConcurrencyPolicy.validated(
      maximumConcurrentClaims
    ) ?? NativeLocalAgentConcurrencyPolicy.adaptiveDefault
  }

  public func configure(ownerUserID: String) async {
    try? await configure(ownerUserID: ownerUserID, externalMCPConfigs: [])
  }

  public func configure(
    ownerUserID: String,
    externalMCPConfigs: [NativeLocalAgentExternalMCPConfig]
  ) async throws {
    let cancelledPollingTask = resetLocked()
    await executor.reset()
    await cancelledPollingTask?.value
    try await executor.configureExternalMCPs(externalMCPConfigs)
    self.ownerUserID = ownerUserID
    // A model can spend longer than the short bootstrap claim window before
    // publishing its first platform-tool batch. Subscribe immediately so that
    // late batches wake a fresh bounded poll instead of remaining unclaimed.
    startEventMonitoring(ownerUserID: ownerUserID)
    // The Rust scheduler and this native worker start independently. Keep a
    // short reconciliation window so Tasks materialized just after bootstrap
    // cannot leave their first platform tool invocation pending forever.
    schedulePolling(ownerUserID: ownerUserID, maximumIdleDuration: activityWindow)
  }

  public func reset() async {
    let cancelledPollingTask = resetLocked()
    await executor.reset()
    await cancelledPollingTask?.value
    ownerUserID = nil
  }

  /// Releases resources owned by a terminal Run before a retry creates its successor.
  /// Browser CDP owns an exclusive Chrome Bridge, so waiting for idle expiration would
  /// leave the replacement Run with a paired mode but no usable bridge backend.
  public func releaseRun(_ runID: String) async {
    await executor.release(runID: runID)
  }

  /// Starts a bounded reconciliation poll. The account-scoped event listener
  /// remains active independently, so arbitrarily long model think time can
  /// still wake a fresh poll when the next platform-tool batch is published.
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
    guard eventWakeTask == nil else { return }
    eventWakeTask = Task { [weak self, eventHub] in
      await eventHub.configure(ownerUserID: ownerUserID)
      let updates = await eventHub.updates()
      for await update in updates {
        guard !Task.isCancelled,
          update.ownerUserID == ownerUserID
        else { continue }
        guard case .events(let events) = update.kind else { continue }
        await self?.handleEventUpdate(
          events,
          ownerUserID: ownerUserID,
          generation: expectedGeneration
        )
      }
    }
  }

  private func handleEventUpdate(
    _ events: [LocalAgentEventRecord],
    ownerUserID: String,
    generation: UUID
  ) async {
    guard self.ownerUserID == ownerUserID,
      self.generation == generation
    else { return }

    var releasedRunIDs = Set<String>()
    for event in events where
      NativeLocalAgentPlatformToolPollingPolicy.shouldReleaseRunResources(
        forEventType: event.eventType
      ) && releasedRunIDs.insert(event.runID).inserted
    {
      await executor.release(runID: event.runID)
    }

    if NativeLocalAgentPlatformToolPollingPolicy.shouldWake(
      forEventTypes: events.map(\.eventType)
    ) {
      wake()
    }
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
    await withTaskGroup(of: PollGroupResult.self) { group in
      var activeClaimCount = 0
      var refillPending = false
      while !Task.isCancelled,
        generation == expectedGeneration,
        self.ownerUserID == ownerUserID
      {
        do {
          if try await approvalHandler?.resolveNextPending(
            ownerUserID: ownerUserID
          ) == true {
            // A restored approval may be the first visible sign that a run is
            // still active after app launch. Keep listening before the host
            // resumes the model so later tool batches do not require another
            // manual wake or app restart.
            startEventMonitoring(ownerUserID: ownerUserID)
            idleDelay = .milliseconds(250)
            continue
          }
          while activeClaimCount < maximumConcurrentClaims,
            !Task.isCancelled,
            generation == expectedGeneration,
            self.ownerUserID == ownerUserID
          {
            guard let claim = try await client.claimNext(
              ownerUserID: ownerUserID,
              workerID: workerID,
              leaseDurationMilliseconds: claimLeaseDurationMilliseconds
            ) else { break }
            // Bootstrap reconciliation may discover persisted work before the
            // event monitor is active. Once it does, subscribe for later tool
            // batches in the same Run.
            startEventMonitoring(ownerUserID: ownerUserID)
            activeClaimCount += 1
            group.addTask { [weak self] in
              guard let self else { return .claimFinished(false) }
              return .claimFinished(
                await self.executeAndCommit(
                  ownerUserID: ownerUserID,
                  claim: claim,
                  generation: expectedGeneration
                ))
            }
          }
          if activeClaimCount > 0 {
            // New tool batches can arrive while an earlier Plugin call is still
            // running. Keep a lightweight refill waiter in the group whenever
            // capacity remains; otherwise group.next() would wait only for the
            // long-running call and later independent claims would stay pending.
            if activeClaimCount < maximumConcurrentClaims, !refillPending {
              refillPending = true
              group.addTask {
                do {
                  try await Task.sleep(for: .milliseconds(250))
                } catch {}
                return .refill
              }
            }
            if let result = await group.next() {
              switch result {
              case .claimFinished(let committed):
                activeClaimCount -= 1
                if committed {
                  idleDelay = .milliseconds(250)
                } else if maximumIdleDuration == .zero {
                  group.cancelAll()
                  await group.waitForAll()
                  return
                }
              case .refill:
                refillPending = false
              }
              continue
            }
          }
        } catch is CancellationError {
          group.cancelAll()
          await group.waitForAll()
          return
        } catch {
          if maximumIdleDuration == .zero {
            group.cancelAll()
            await group.waitForAll()
            return
          }
        }
        guard maximumIdleDuration != .zero, clock.now < deadline else {
          group.cancelAll()
          await group.waitForAll()
          return
        }
        do {
          try await Task.sleep(for: idleDelay)
        } catch {
          group.cancelAll()
          await group.waitForAll()
          return
        }
        idleDelay = min(idleDelay * 2, .seconds(2))
      }
      group.cancelAll()
      await group.waitForAll()
    }
  }

  private func executeAndCommit(
    ownerUserID: String,
    claim: LocalAgentToolClaim,
    generation expectedGeneration: UUID
  ) async -> Bool {
    guard
      let outcome = await executeWhileRenewing(
        ownerUserID: ownerUserID,
        claim: claim
      ),
      !Task.isCancelled,
      generation == expectedGeneration,
      self.ownerUserID == ownerUserID
    else { return false }
    do {
      _ = try await client.commit(
        ownerUserID: ownerUserID,
        claim: claim,
        outcome: outcome
      )
      // Long-running commands can outlive the monitoring window. Refresh it
      // at the commit boundary before the model requests another tool.
      startEventMonitoring(ownerUserID: ownerUserID)
      return true
    } catch {
      return false
    }
  }

  private func executeWhileRenewing(
    ownerUserID: String,
    claim: LocalAgentToolClaim
  ) async -> LocalAgentToolOutcome? {
    if let preflightOutcome = claim.preflightOutcome {
      return preflightOutcome
    }
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
      if case let .object(values) = output,
         values["is_error"] == .bool(true) || values["isError"] == .bool(true) {
        // A returned tool error is a known failure, unlike a thrown transport error
        // where a side effect may already have happened. Preserve its structured
        // output, but never commit it as a successful Skill activation.
        return .failed(error: "native_tool_reported_error", detail: output)
      }
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

  private func resetLocked() -> Task<Void, Never>? {
    let task = pollingTask
    generation = UUID()
    pollingTask?.cancel()
    pollingTask = nil
    eventWakeTask?.cancel()
    eventWakeTask = nil
    pendingWake = false
    return task
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
