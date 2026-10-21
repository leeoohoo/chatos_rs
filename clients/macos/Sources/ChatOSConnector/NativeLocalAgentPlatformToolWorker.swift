import ChatOSCore
import Foundation
import OSLog

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
  private static let logger = Logger(
    subsystem: "com.chatos.swift-client",
    category: "LocalAgentToolLease"
  )

  private enum ClaimLeaseLoss: Sendable {
    case rejected
    case renewalUnavailable(String)
  }

  private enum ClaimExecutionResult: Sendable {
    case outcome(LocalAgentToolOutcome)
    case leaseLost(ClaimLeaseLoss)
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
  private var approvalTask: Task<Void, Never>?
  private var eventWakeTask: Task<Void, Never>?
  private var pendingWake = false
  private var pendingApprovalWake = false

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
      ),
      connector: connector
    )
    approvalHandler = NativeLocalAgentToolApprovalHandler(
      host: host,
      projects: projects,
      connector: connector,
      editEvidenceProvider: writeStore,
      decisionJournalURL: attachmentRootURL.deletingLastPathComponent()
        .appendingPathComponent("approval-writebacks.json", isDirectory: false)
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
    let cancelledTasks = resetLocked()
    await executor.reset()
    for task in cancelledTasks { await task.value }
    try await executor.configureExternalMCPs(externalMCPConfigs)
    self.ownerUserID = ownerUserID
    // A model can spend longer than the short bootstrap claim window before
    // publishing its first platform-tool batch. Subscribe immediately so that
    // late batches wake a fresh bounded poll instead of remaining unclaimed.
    startEventMonitoring(ownerUserID: ownerUserID)
    // The Rust scheduler and this native worker start independently. Keep a
    // short reconciliation window so Tasks materialized just after bootstrap
    // cannot leave their first platform tool invocation pending forever.
    scheduleApprovalResolution(ownerUserID: ownerUserID)
    schedulePolling(ownerUserID: ownerUserID, maximumIdleDuration: activityWindow)
  }

  public func reset() async {
    let cancelledTasks = resetLocked()
    await executor.reset()
    for task in cancelledTasks { await task.value }
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
    scheduleApprovalResolution(ownerUserID: ownerUserID)
    guard pollingTask == nil else {
      pendingWake = true
      return
    }
    schedulePolling(ownerUserID: ownerUserID, maximumIdleDuration: activityWindow)
  }

  private func scheduleApprovalResolution(ownerUserID: String) {
    guard approvalHandler != nil else { return }
    guard approvalTask == nil else {
      pendingApprovalWake = true
      return
    }
    let token = generation
    let maximumIdleDuration = activityWindow
    approvalTask = Task { [weak self] in
      await self?.resolveApprovals(
        ownerUserID: ownerUserID,
        generation: token,
        maximumIdleDuration: maximumIdleDuration
      )
    }
  }

  private func resolveApprovals(
    ownerUserID: String,
    generation expectedGeneration: UUID,
    maximumIdleDuration: Duration
  ) async {
    let clock = ContinuousClock()
    let deadline = clock.now.advanced(by: maximumIdleDuration)
    var idleDelay = Duration.milliseconds(250)
    defer {
      if generation == expectedGeneration {
        approvalTask = nil
        if pendingApprovalWake, let ownerUserID = self.ownerUserID {
          pendingApprovalWake = false
          scheduleApprovalResolution(ownerUserID: ownerUserID)
        }
      }
    }
    while !Task.isCancelled,
      generation == expectedGeneration,
      self.ownerUserID == ownerUserID
    {
      do {
        if try await approvalHandler?.resolveNextPending(ownerUserID: ownerUserID) == true {
          idleDelay = .milliseconds(250)
          continue
        }
      } catch is CancellationError {
        return
      } catch {}
      guard maximumIdleDuration != .zero, clock.now < deadline else { return }
      do {
        try await Task.sleep(for: idleDelay)
      } catch {
        return
      }
      idleDelay = min(idleDelay * 2, .seconds(2))
    }
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
    let leaseDuration = Duration.milliseconds(Int64(min(
      leaseDurationMilliseconds,
      UInt64(Int64.max)
    )))
    let currentUnixMilliseconds = Int64(Date().timeIntervalSince1970 * 1_000)
    let initialLeaseRemaining = claim.invocation.claimUntilUnixMs.map {
      Duration.milliseconds(max(0, $0 - currentUnixMilliseconds))
    } ?? leaseDuration
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
        let clock = ContinuousClock()
        var knownValidUntil = clock.now.advanced(by: initialLeaseRemaining)
        var nextDelay = min(heartbeatInterval, initialLeaseRemaining)
        do {
          while !Task.isCancelled {
            try await Task.sleep(for: nextDelay)
            try Task.checkCancellation()
            do {
              let renewalStartedAt = clock.now
              guard try await client.renew(
                ownerUserID: ownerUserID,
                claim: claim,
                leaseDurationMilliseconds: leaseDurationMilliseconds
              ) else {
                return .leaseLost(.rejected)
              }
              // The Host starts the renewed lease while handling this call. Measuring from
              // request start is conservative and never overstates the known-valid window.
              knownValidUntil = renewalStartedAt.advanced(by: leaseDuration)
              nextDelay = min(heartbeatInterval, leaseDuration)
            } catch is CancellationError {
              throw CancellationError()
            } catch {
              let errorType = String(reflecting: type(of: error))
              let now = clock.now
              guard now < knownValidUntil else {
                return .leaseLost(.renewalUnavailable(errorType))
              }
              let remaining = now.duration(to: knownValidUntil)
              nextDelay = min(.milliseconds(250), remaining)
              Self.logger.warning(
                "Tool claim renewal temporarily unavailable; retrying before lease expiry. invocation=\(claim.invocation.invocationID, privacy: .public) error_type=\(errorType, privacy: .public)"
              )
            }
          }
        } catch is CancellationError {
          return .leaseLost(.renewalUnavailable("cancelled"))
        } catch {
          return .leaseLost(.renewalUnavailable(String(reflecting: type(of: error))))
        }
        return .leaseLost(.renewalUnavailable("heartbeat_stopped"))
      }

      guard let first = await group.next() else {
        group.cancelAll()
        return nil
      }
      group.cancelAll()
      switch first {
      case .outcome(let outcome): return outcome
      case .leaseLost(let reason):
        switch reason {
        case .rejected:
          Self.logger.error(
            "Tool claim renewal was rejected; cancelling execution. invocation=\(claim.invocation.invocationID, privacy: .public)"
          )
        case .renewalUnavailable(let errorType):
          Self.logger.error(
            "Tool claim could not be renewed before expiry; cancelling execution. invocation=\(claim.invocation.invocationID, privacy: .public) error_type=\(errorType, privacy: .public)"
          )
        }
        return nil
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
      let requiresReview = claim.invocation.sideEffecting
        && (error as? NativeLocalAgentPlatformToolError)?.isKnownPreEffectFailure != true
      return requiresReview
        ? .needsReview(reason: summary, detail: detail)
        : .failed(error: summary, detail: detail)
    }
  }

  private func resetLocked() -> [Task<Void, Never>] {
    let tasks = [pollingTask, approvalTask].compactMap { $0 }
    generation = UUID()
    pollingTask?.cancel()
    pollingTask = nil
    approvalTask?.cancel()
    approvalTask = nil
    eventWakeTask?.cancel()
    eventWakeTask = nil
    pendingWake = false
    pendingApprovalWake = false
    return tasks
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
  case projectToolRejected(String)
  case projectToolFailed
  case pluginInstallerRejected(String)
  case pluginInstallerFileOperationFailed(Int)
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
    case .projectToolRejected(let reason): reason
    case .projectToolFailed: "The local project tool arguments or state are invalid."
    case .pluginInstallerRejected(let reason): "Plugin 安装失败：\(reason)"
    case .pluginInstallerFileOperationFailed(let code):
      "Plugin 安装文件操作失败（错误码 \(code)），旧安装已保留。"
    case .approvalRequired: "The local project change requires approval before execution."
    case .capabilityNotSelected: "This Task did not select the requested local capability."
    }
  }

  /// These failures are known to occur before a native mutation is committed. They are
  /// ordinary tool results that the model can correct, not evidence of an unknown side effect.
  var isKnownPreEffectFailure: Bool {
    switch self {
    case .projectToolFailed, .pluginInstallerFileOperationFailed:
      false
    case .unsupportedTool, .invalidArguments, .invalidField, .invalidRunContext,
         .attachmentNotAuthorized, .projectUnavailable, .projectToolRejected,
         .pluginInstallerRejected, .approvalRequired, .capabilityNotSelected:
      true
    }
  }
}
