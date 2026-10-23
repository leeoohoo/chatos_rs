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
  private let pluginInstallerTools: NativeLocalAgentPluginInstallerToolExecutor?

  init(
    host: any LocalAgentHostClientServicing,
    attachmentRootURL: URL,
    projectTools: (any NativeLocalAgentProjectToolExecuting)? = nil,
    attachmentWorkspaceResolver: (any NativeLocalAgentAttachmentWorkspaceResolving)? = nil,
    connector: NativeLocalConnectorService? = nil
  ) {
    runtime = .init(host: host)
    conversations = .init(host: host)
    attachmentVault = .init(rootURL: attachmentRootURL)
    self.projectTools = projectTools
    self.attachmentWorkspaceResolver = attachmentWorkspaceResolver
    externalMCPs = .init(host: host)
    pluginInstallerTools = connector.map {
      NativeLocalAgentPluginInstallerToolExecutor(host: host, connector: $0)
    }
  }

  func execute(
    ownerUserID: String,
    invocation: LocalAgentToolInvocationRecord
  ) async throws -> LocalAgentJSONValue {
    if invocation.toolName.hasPrefix(
      NativeLocalAgentPlatformToolCatalog.pluginInstallerToolPrefix
    ) {
      guard let pluginInstallerTools else {
        throw NativeLocalAgentPlatformToolError.unsupportedTool
      }
      return try await pluginInstallerTools.execute(
        ownerUserID: ownerUserID,
        invocation: invocation
      )
    }
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
    let attachment = try await conversations.attachment(
      ownerUserID: ownerUserID,
      conversationID: conversationID,
      authorizedLocalRef: authorizedLocalRef
    )
    guard let attachment else {
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
        return try await NativeLocalAgentAttachmentModelInput.result(
          data,
          record: attachment,
          taskID: invocation.runID
        )
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
          data,
          record: attachment,
          imported: imported,
          taskID: invocation.runID
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
