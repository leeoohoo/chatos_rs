import ChatOSAgentRuntime
import ChatOSCore
import Foundation

struct NativeLocalAgentTeamExecutorOutcome: Sendable, Equatable {
  enum State: Sendable, Equatable {
    case completed
    case suspended
    case failed
  }

  let state: State
  let detail: String?
}
/// Starts Agent-team Todo work on the same durable Rust Local Agent Host used by normal Tasks.
/// Team boards, members, deliveries and acceptance gates remain native collaboration objects.
public actor NativeLocalAgentTeamExecutor {
  private struct Configuration: Sendable {
    let ownerUserID: String
    let models: [String: LocalAgentModelConfigSnapshot]
    let capabilities: [String: LocalAgentCapabilityPolicySnapshot]
  }

  private let conversations: NativeLocalAgentConversationClient
  private let runtime: NativeLocalAgentRuntimeClient
  private let platformToolWorker: NativeLocalAgentPlatformToolWorker?
  private let groupChatService: NativeAgentGroupChatService
  private var configuration: Configuration?

  public init(
    host: any LocalAgentHostClientServicing,
    groupChatService: NativeAgentGroupChatService,
    platformToolWorker: NativeLocalAgentPlatformToolWorker? = nil
  ) {
    conversations = .init(host: host)
    runtime = .init(host: host)
    self.groupChatService = groupChatService
    self.platformToolWorker = platformToolWorker
  }

  func configure(
    ownerUserID: String,
    bootstrap: NativeLocalAgentBootstrapResult
  ) throws {
    let models = Dictionary(uniqueKeysWithValues: bootstrap.modelSnapshots.map {
      ($0.modelConfigRef, $0)
    })
    guard !models.isEmpty,
      bootstrap.modelSnapshots.allSatisfy({ $0.ownerUserID == ownerUserID }),
      bootstrap.capabilitySnapshotsByModelConfigRef.keys.allSatisfy({ models[$0] != nil })
    else {
      throw NativeLocalAgentConversationServiceError.notConfigured
    }
    configuration = .init(
      ownerUserID: ownerUserID,
      models: models,
      capabilities: bootstrap.capabilitySnapshotsByModelConfigRef
    )
  }

  func reset() {
    configuration = nil
  }

  func execute(
    profile: LocalAgentProfile,
    room: ProjectAgentRoom,
    member: ProjectAgentRoomMember,
    delivery: ProjectAgentDelivery,
    todo: LocalAgentTodo,
    groupRun: LocalAgentGroupChatRun
  ) async throws -> NativeLocalAgentTeamExecutorOutcome {
    let config = try configured(ownerUserID: room.ownerUserID)
    guard delivery.lane == .executor,
      delivery.triggerKind == .todo,
      delivery.targetAgentID == profile.id,
      delivery.roomID == room.id,
      member.agentID == profile.id,
      member.roomID == room.id,
      todo.agentID == profile.id,
      todo.teamRoomID == room.id,
      groupRun.context.deliveryID == delivery.id,
      groupRun.context.agentID == profile.id,
      groupRun.context.roomID == room.id,
      groupRun.context.projectID == room.projectID,
      let model = config.models[groupRun.modelConfigID],
      let capability = config.capabilities[groupRun.modelConfigID]
    else {
      throw NativeLocalAgentPlatformToolError.invalidRunContext
    }

    let conversationID = "agent-team-executor-\(delivery.id)"
    var conversation = try await ensureConversation(
      ownerUserID: room.ownerUserID,
      conversationID: conversationID,
      title: "\(room.draft.name) · \(todo.title)",
      projectID: room.projectID
    )
    let baseRunID = "agent-team-run-\(groupRun.id.uuidString.lowercased())"
    // Reconcile the latest deterministic Host Run after an app restart instead of creating a
    // duplicate turn. A conversation with N turns resumes attempt N; a fresh one starts at 1.
    var attempts = max(0, conversation.turns.count - 1)
    while attempts < 8 {
      try Task.checkCancellation()
      let attempt = attempts + 1
      let hostRunID = attempt == 1 ? baseRunID : "\(baseRunID)-\(attempt)"
      let turnID = "agent-team-turn-\(groupRun.id.uuidString.lowercased())-\(attempt)"
      let hostRun: LocalAgentRunRecord
      if let existing = try await existingRun(
        ownerUserID: room.ownerUserID,
        runID: hostRunID
      ) {
        hostRun = existing
      } else {
        let metadata = try messageMetadata(
          profile: profile,
          room: room,
          member: member,
          delivery: delivery,
          todo: todo,
          groupRun: groupRun
        )
        let message = attempt == 1
          ? Self.initialPrompt(todo: todo, groupRun: groupRun)
          : Self.continuationPrompt
        let mutation = try await conversations.startTurn(.init(
          ownerUserID: room.ownerUserID,
          conversationID: conversationID,
          expectedConversationVersion: conversation.conversation.version,
          conversationIfMissing: conversation.conversation.version == 1
            && conversation.turns.isEmpty
            ? .init(
              title: "\(room.draft.name) · \(todo.title)",
              resource: .init(kind: .project, resourceID: room.projectID)
            )
            : nil,
          turnID: turnID,
          messageID: "agent-team-message-\(groupRun.id.uuidString.lowercased())-\(attempt)",
          runID: hostRunID,
          message: message,
          messageMetadata: metadata,
          modelConfigRef: model.modelConfigRef,
          modelConfigRevision: model.modelConfigRevision,
          capabilityPolicyRevision: capability.capabilityPolicyRevision,
          maxIterations: UInt32(min(10_000, max(2, groupRun.policy.maximumModelCalls)))
        ))
        await platformToolWorker?.wake()
        hostRun = try await runtime.run(
          ownerUserID: room.ownerUserID,
          runID: mutation.turn.runID
        )
        conversation = try await conversations.context(
          ownerUserID: room.ownerUserID,
          conversationID: conversationID
        )
      }

      let terminal: LocalAgentRunRecord
      do {
        terminal = try await waitForTerminal(
          ownerUserID: room.ownerUserID,
          run: hostRun
        )
      } catch is CancellationError {
        let detail = try? await conversations.context(
          ownerUserID: room.ownerUserID,
          conversationID: conversationID
        )
        if let turn = detail?.turns.first(where: { $0.runID == hostRunID }) {
          _ = try? await conversations.cancelTurn(
            ownerUserID: room.ownerUserID,
            conversationID: conversationID,
            expectedConversationVersion: detail?.conversation.version ?? 0,
            turnID: turn.turnID,
            expectedRunVersion: nil,
            reason: "Agent team delivery cancelled"
          )
        }
        throw CancellationError()
      }
      let store = try await groupChatService.store()
      let currentDelivery = try await store.delivery(
        ownerUserID: room.ownerUserID,
        deliveryID: delivery.id
      )
      let currentTodo = try await store.todoForDelivery(
        ownerUserID: room.ownerUserID,
        deliveryID: delivery.id
      )
      if currentDelivery?.status == .completed || currentDelivery?.status == .cancelled {
        let detail = Self.nonempty(currentTodo?.result)
          ?? Self.nonempty(currentTodo?.blockedReason)
          ?? Self.nonempty(terminal.terminalOutcome?.conversationDisplayText)
        return .init(state: .completed, detail: detail)
      }
      switch terminal.status {
      case "succeeded":
        guard let currentTodo else {
          return .init(state: .failed, detail: "Host Run 结束后无法重新读取绑定 Todo。")
        }
        if Self.shouldContinueAfterSuccessfulHostRun(
          acceptanceChecks: currentTodo.acceptanceChecks
        ) {
          attempts = attempt
          conversation = try await conversations.context(
            ownerUserID: room.ownerUserID,
            conversationID: conversationID
          )
          continue
        }
        return .init(
          state: .failed,
          detail: "Agent 已结束 Host Run，但没有通过 todo_complete 或 todo_block 结束绑定任务。"
        )
      case "waiting_user", "paused", "needs_review":
        return .init(
          state: .suspended,
          detail: Self.nonempty(terminal.terminalOutcome?.conversationDisplayText)
            ?? "团队 Executor 已暂停，等待恢复或检查。"
        )
      case "cancelled":
        return .init(state: .suspended, detail: "团队 Executor Host Run 已取消。")
      default:
        return .init(
          state: .failed,
          detail: Self.nonempty(terminal.terminalOutcome?.conversationDisplayText)
            ?? "团队 Executor Host Run 失败。"
        )
      }
    }
    return .init(
      state: .failed,
      detail: "Agent 连续总结但仍有可执行验收项，已达到自动续跑上限。"
    )
  }

  private func configured(ownerUserID: String) throws -> Configuration {
    guard let configuration, configuration.ownerUserID == ownerUserID else {
      throw NativeLocalAgentConversationServiceError.notConfigured
    }
    return configuration
  }

  private func ensureConversation(
    ownerUserID: String,
    conversationID: String,
    title: String,
    projectID: String
  ) async throws -> LocalAgentConversationDetail {
    do {
      let existing = try await conversations.get(
        ownerUserID: ownerUserID,
        conversationID: conversationID
      )
      guard existing.conversation.resource == .init(kind: .project, resourceID: projectID) else {
        throw NativeLocalAgentPlatformToolError.invalidRunContext
      }
      return existing
    } catch let error as NativeLocalAgentHostError {
      guard case let .hostError(code, _, _) = error, code == "not_found" else { throw error }
    }
    // A fresh team executor conversation is inserted by start_conversation_turn in the
    // same database transaction as its first Turn. Keeping creation and start in one Host
    // command prevents a Host restart or package replacement from leaving a durable gap.
    return .init(
      conversation: .init(
        conversationID: conversationID,
        ownerUserID: ownerUserID,
        title: title,
        resource: .init(kind: .project, resourceID: projectID),
        version: 1,
        createdAtUnixMs: 0,
        updatedAtUnixMs: 0
      ),
      turns: [],
      messages: [],
      attachments: []
    )
  }

  private func existingRun(
    ownerUserID: String,
    runID: String
  ) async throws -> LocalAgentRunRecord? {
    do {
      return try await runtime.run(ownerUserID: ownerUserID, runID: runID)
    } catch let error as NativeLocalAgentHostError {
      guard case let .hostError(code, _, _) = error, code == "not_found" else { throw error }
      return nil
    }
  }

  private func waitForTerminal(
    ownerUserID: String,
    run initial: LocalAgentRunRecord
  ) async throws -> LocalAgentRunRecord {
    var run = initial
    var cursor = try await runtime.latestEventCursor(ownerUserID: ownerUserID)
    while !Self.isTerminal(run.status) {
      try Task.checkCancellation()
      let page = try await runtime.waitEvents(
        ownerUserID: ownerUserID,
        afterCursor: cursor,
        timeoutMilliseconds: 20_000,
        payloadMode: .routing
      )
      cursor = page.nextCursor
      run = try await runtime.run(ownerUserID: ownerUserID, runID: run.runID)
    }
    return run
  }

  private static func isTerminal(_ status: String) -> Bool {
    ["succeeded", "failed", "cancelled", "waiting_user", "paused", "needs_review"]
      .contains(status)
  }

  private static func nonempty(_ value: String?) -> String? {
    guard let value else { return nil }
    let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines)
    return normalized.isEmpty ? nil : normalized
  }

  /// A Human-only blocker is terminal for model work, but it never hides another independent
  /// acceptance item that the Executor can still act on.
  static func shouldContinueAfterSuccessfulHostRun(
    acceptanceChecks: [LocalAgentTodoAcceptanceCheck]
  ) -> Bool {
    !AgentExecutionAcceptanceGate.actionableChecks(
      acceptanceChecks.map(\.gateCheck)
    ).isEmpty
  }

  private func messageMetadata(
    profile: LocalAgentProfile,
    room: ProjectAgentRoom,
    member: ProjectAgentRoomMember,
    delivery: ProjectAgentDelivery,
    todo: LocalAgentTodo,
    groupRun: LocalAgentGroupChatRun
  ) throws -> LocalAgentJSONValue {
    guard let snapshot = groupRun.progressiveSkillSnapshot,
      let profession = snapshot.skills.first(where: { $0.kind == .profession })
    else {
      throw NativeLocalAgentPlatformToolError.invalidRunContext
    }
    let plugins = try todo.executionPlan.plugins.map { plugin -> LocalAgentJSONValue in
      guard !plugin.pluginKey.isEmpty,
        !plugin.releaseID.isEmpty,
        !plugin.version.isEmpty,
        !plugin.artifactSHA256.isEmpty
      else {
        throw NativeLocalAgentPlatformToolError.invalidRunContext
      }
      return .object([
        "installationId": .string(plugin.pluginID),
        "pluginId": .string(plugin.pluginKey),
        "componentKey": .string(plugin.componentKey),
        "componentRevision": .string(plugin.releaseID),
        "releaseId": .string(plugin.releaseID),
        "releaseDigest": .string(plugin.artifactSHA256),
        "manifestVersion": .string(plugin.version),
        "displayName": .string(plugin.displayName),
        "reason": .string(plugin.reason),
      ])
    }
    return .object([
      "source": .string("native_agent_team"),
      "agent_execution_context": .object([
        "schema_version": .number(2),
        "room_id": .string(room.id),
        "agent_id": .string(profile.id),
        "agent_name": .string(profile.draft.name),
        "role_prompt": .string(profile.draft.rolePrompt),
        "profession_key": .string(profession.key),
        "profession_label": .string(profession.label),
        "profession_skill_ref": .string(profession.skillRef),
        "profession_skill_sha256": .string(profession.instructionsSHA256),
        "profession_skill_markdown": .string(profession.instructions),
        "default_plugin_ids": .array(profile.draft.defaultPluginIDs.map(LocalAgentJSONValue.string)),
        "default_skill_ids": .array(profile.draft.defaultSkillIDs.map(LocalAgentJSONValue.string)),
        "project_id": .string(room.projectID),
        "team_name": .string(room.draft.name),
        "team_goal": .string(room.draft.goal),
        "team_role": .string(member.draft.role),
        "team_responsibility": .string(member.draft.responsibility),
        "is_project_manager": .bool(room.projectManagerAgentID == profile.id),
      ]),
      "agent_team_execution": .object([
        "schema_version": .number(1),
        "delivery_lane": .string("executor"),
        "trigger_kind": .string("todo"),
        "delivery_id": .string(delivery.id),
        "todo_id": .string(todo.id),
        "team_id": .string(room.id),
        "agent_id": .string(profile.id),
        "project_id": .string(room.projectID),
        "execution_plan": .object([
          "requires_execution": .bool(todo.executionPlan.requiresExecution),
          "builtin_capabilities": .array(
            todo.executionPlan.builtinCapabilities.map { .string($0.rawValue) }
          ),
          "plugins": .array(plugins),
        ]),
      ]),
    ])
  }

  private static func initialPrompt(
    todo: LocalAgentTodo,
    groupRun: LocalAgentGroupChatRun
  ) -> String {
    let hookContext = groupRun.checkpoint.messages.compactMap { message -> String? in
      guard message.role == .system,
        message.content.contains("\"type\":\"trusted_plugin_hook_context\"")
      else { return nil }
      return message.content
    }.joined(separator: "\n")
    let suffix = hookContext.isEmpty ? "" : "\n\n[已批准 Plugin Hook 上下文]\n\(hookContext)"
    return """
    执行当前已绑定的团队 Todo：\(todo.title)

    \(todo.detail)

    先调用 todo_get_context 读取完整事实、来源、进度和验收项。持续完成所有仍可执行的验收项，并逐项调用 todo_acceptance_update；只有全部通过才调用 todo_complete。确实只能由 Human 处理的单项标记 blocked + requires_human=true，但不得因此跳过其他独立项目。
    """ + suffix
  }

  private static let continuationPrompt = """
    你上一轮给出了总结，但绑定 Todo 仍未结束，并且仍有可执行验收项。重新调用 todo_get_context 读取最新状态，不要重复总结；继续完成所有未通过且不属于 Human-only blocked 的项目，逐项记录证据，满足门禁后再调用 todo_complete 或 todo_block。
    """
}
