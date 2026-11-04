import ChatOSAgentRuntime
import ChatOSCore
import Foundation

/// Bridges only the collaboration tools of a Rust Host `task_execution` Run back into the
/// existing identity-bound Agent-team store. Authority always comes from normalized Host input
/// plus current durable records; model arguments never contain a real Todo, Agent or team ID.
actor NativeLocalAgentTeamExecutorToolRouter {
  private struct BoundExecution {
    let context: LocalAgentChatRunContext
    let todoID: String
    let cacheKey: String
  }

  private let service: NativeAgentGroupChatService
  private let relay: LocalAgentRelayMCPServer
  private var providers: [String: LocalAgentChatToolProvider] = [:]

  init(service: NativeAgentGroupChatService) {
    self.service = service
    relay = LocalAgentRelayMCPServer(service: service)
  }

  func execute(
    ownerUserID: String,
    run: LocalAgentRunRecord,
    invocation: LocalAgentToolInvocationRecord
  ) async throws -> LocalAgentJSONValue {
    guard NativeLocalAgentPlatformToolCatalog.teamExecutorToolNames.contains(
      invocation.toolName
    ) else {
      throw NativeLocalAgentPlatformToolError.unsupportedTool
    }
    let bound = try await boundExecution(ownerUserID: ownerUserID, run: run)
    let provider: LocalAgentChatToolProvider
    if let cached = providers[bound.cacheKey] {
      provider = cached
    } else {
      let connected = try await relay.connect(context: bound.context)
      providers[bound.cacheKey] = connected
      provider = connected
    }
    let argumentData = try JSONEncoder().encode(invocation.arguments)
    guard let argumentJSON = String(data: argumentData, encoding: .utf8) else {
      throw NativeLocalAgentPlatformToolError.invalidArguments
    }
    let outcome = try await provider.executeFromNativeTeamExecutor(.init(
      id: invocation.callID,
      name: invocation.toolName,
      arguments: argumentJSON
    ))
    let content = (try? JSONDecoder().decode(
      LocalAgentJSONValue.self,
      from: Data(outcome.content.utf8)
    )) ?? .string(outcome.content)
    await service.publishChange(.init(
      ownerUserID: ownerUserID,
      roomID: bound.context.roomID,
      agentID: bound.context.agentID,
      kind: .roomUpdated
    ))
    guard outcome.isError else { return content }
    return .object([
      "is_error": .bool(true),
      "made_progress": .bool(outcome.madeProgress),
      "detail": content,
    ])
  }

  func appendProjectToolAudit(
    ownerUserID: String,
    run: LocalAgentRunRecord,
    invocation: LocalAgentToolInvocationRecord,
    output: LocalAgentJSONValue
  ) async throws {
    let bound = try await boundExecution(ownerUserID: ownerUserID, run: run)
    guard let audit = Self.audit(
      toolName: invocation.toolName,
      arguments: Self.object(invocation.arguments) ?? [:],
      output: Self.object(output) ?? [:]
    ) else { return }
    let store = try await service.store()
    _ = try await store.appendAgentTodoProgress(
      ownerUserID: ownerUserID,
      agentID: bound.context.agentID,
      todoID: bound.todoID,
      kind: .progress,
      runID: bound.context.runID,
      stage: audit.stage,
      detail: String(audit.detail.prefix(16_000)),
      assetUpdateSuggestions: [],
      nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
    )
    await service.publishChange(.init(
      ownerUserID: ownerUserID,
      roomID: bound.context.roomID,
      agentID: bound.context.agentID,
      kind: .roomUpdated
    ))
  }

  func reset() {
    providers.removeAll(keepingCapacity: false)
  }

  private func boundExecution(
    ownerUserID: String,
    run: LocalAgentRunRecord
  ) async throws -> BoundExecution {
    guard run.ownerUserID == ownerUserID,
      run.profileKey == "task_execution",
      let input = Self.object(run.input),
      let executor = Self.object(input["team_executor_context"]),
      let identity = Self.object(input["agent_execution_context"]),
      identity["schema_version"] == .number(2),
      let deliveryID = Self.string(executor["delivery_id"]),
      let todoID = Self.string(executor["todo_id"]),
      let teamID = Self.string(executor["team_id"]),
      let agentID = Self.string(executor["agent_id"]),
      let projectID = Self.string(executor["project_id"]),
      Self.string(identity["room_id"]) == teamID,
      Self.string(identity["agent_id"]) == agentID,
      Self.string(identity["project_id"]) == projectID,
      Self.string(input["conversation_id"]) == "agent-team-executor-\(deliveryID)"
    else {
      throw NativeLocalAgentPlatformToolError.invalidRunContext
    }
    let store = try await service.store()
    guard let delivery = try await store.delivery(
      ownerUserID: ownerUserID,
      deliveryID: deliveryID
    ), delivery.lane == .executor,
      delivery.targetAgentID == agentID,
      delivery.roomID == teamID,
      let todo = try await store.todoForDelivery(
        ownerUserID: ownerUserID,
        deliveryID: deliveryID
      ), todo.id == todoID,
      todo.agentID == agentID,
      todo.teamRoomID == teamID,
      let room = try await store.room(ownerUserID: ownerUserID, roomID: teamID),
      room.projectID == projectID,
      let groupRun = try await store.run(
        ownerUserID: ownerUserID,
        deliveryID: deliveryID
      ), groupRun.context.ownerUserID == ownerUserID,
      groupRun.context.projectID == projectID,
      groupRun.context.roomID == teamID,
      groupRun.context.agentID == agentID,
      groupRun.context.deliveryID == deliveryID,
      groupRun.context.lane == .executor
    else {
      throw NativeLocalAgentPlatformToolError.invalidRunContext
    }
    let context = try LocalAgentChatRunContext(
      ownerUserID: ownerUserID,
      projectID: projectID,
      roomID: teamID,
      agentID: agentID,
      deliveryID: deliveryID,
      triggerMessageID: delivery.messageID,
      rootMessageID: delivery.rootMessageID,
      runID: groupRun.context.runID,
      hopCount: delivery.hopCount,
      lane: .executor
    )
    return .init(
      context: context,
      todoID: todoID,
      cacheKey: "\(ownerUserID):\(deliveryID):\(groupRun.context.runID)"
    )
  }

  private static func audit(
    toolName: String,
    arguments: [String: LocalAgentJSONValue],
    output: [String: LocalAgentJSONValue]
  ) -> (stage: String, detail: String)? {
    let result = Self.object(output["result"]) ?? output
    switch toolName {
    case "read_file_raw", "read_file_range", "read_file":
      let path = Self.string(output["path"]) ?? Self.string(arguments["path"]) ?? "(unknown)"
      let hash = Self.string(output["sha256"]).map { "，sha256=\($0)" } ?? ""
      return ("builtin.file_read", "已通过 \(toolName) 读取项目文件：\(path)\(hash)")
    case "list_dir":
      return ("builtin.directory_list", "已通过 list_dir 列出项目目录：\(Self.string(arguments["path"]) ?? ".")")
    case "search_text", "search_files":
      return ("builtin.project_search", "已通过 \(toolName) 搜索项目范围：\(Self.string(arguments["path"]) ?? ".")")
    case "open_edit_session":
      return ("builtin.edit_opened", "已打开事务编辑会话：\(Self.string(result["session_id"]) ?? "(unknown)")")
    case "stage_edit_batch":
      return ("builtin.edit_staged", "已暂存项目文件修改：\(stringList(result["batch_changed_paths"]))")
    case "commit_edit_session":
      return ("builtin.edit_committed", "已提交事务编辑：\(stringList(result["committed_paths"]))")
    case "abort_edit_session":
      return ("builtin.edit_aborted", "已放弃当前事务编辑会话。")
    default:
      return nil
    }
  }

  private static func stringList(_ value: LocalAgentJSONValue?) -> String {
    let values = array(value).compactMap(string)
    return values.isEmpty ? "无实际变化" : values.joined(separator: ", ")
  }

  private static func object(
    _ value: LocalAgentJSONValue?
  ) -> [String: LocalAgentJSONValue]? {
    guard case let .object(result)? = value else { return nil }
    return result
  }

  private static func array(_ value: LocalAgentJSONValue?) -> [LocalAgentJSONValue] {
    guard case let .array(result)? = value else { return [] }
    return result
  }

  private static func string(_ value: LocalAgentJSONValue?) -> String? {
    guard case let .string(raw)? = value else { return nil }
    let result = raw.trimmingCharacters(in: .whitespacesAndNewlines)
    return result.isEmpty ? nil : result
  }
}
