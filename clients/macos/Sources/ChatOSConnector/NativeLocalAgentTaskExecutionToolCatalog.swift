import ChatOSCore
import Foundation

extension NativeLocalAgentPlatformToolCatalog {
  public static var taskExecutionCapabilityTools: [LocalAgentJSONValue] {
    taskExecutionCapabilityTools(externalMCPConfigs: [])
  }

  public static func taskExecutionCapabilityTools(
    externalMCPConfigs: [NativeLocalAgentExternalMCPConfig]
  ) -> [LocalAgentJSONValue] {
    [attachmentReadTool]
    + NativeMCPCodeReadTools.toolDefinitions.compactMap { value in
      guard case .object(let tool) = value,
        case .string(let name)? = tool["name"],
        projectReadOnlyToolNames.contains(name)
      else { return nil }
      return capabilityTool(value)
    }
    + NativeMCPCodeWriteStore.toolDefinitions.map(capabilityTool)
    + NativeMCPTerminalStore.toolDefinitions.compactMap { value in
      guard case .object(let tool) = value,
        case .string(let name)? = tool["name"],
        taskExecutionTerminalToolNames.contains(name)
      else { return nil }
      return capabilityTool(value)
    }
    + NativeMCPRemoteConnectionController.toolDefinitions.map(remoteCapabilityTool)
    + NativeAgentCapabilityBrokerToolCatalog.localAgentCapabilityTools
    + externalMCPConfigs.flatMap { config in
      config.tools.map { tool in
        .object([
          "type": .string("function"),
          "name": .string(tool.publicName),
          "description": .string(tool.description),
          "parameters": tool.inputSchema,
          // The Rust planner removes this local routing marker before the
          // schema is sent to a model. It contains no URL or credential.
          "x-chatos-external-mcp-id": .string(config.resourceID),
        ])
      }
    }
  }

  static var taskExecutionToolNames: Set<String> {
    Set(
      taskExecutionCapabilityTools.compactMap { value in
        guard case .object(let tool) = value,
          case .string(let name)? = tool["name"]
        else { return nil }
        return name
      })
  }

  private static func capabilityTool(_ value: NativeJSONValue) -> LocalAgentJSONValue {
    guard case .object(let tool) = value,
      case .string(let name)? = tool["name"],
      case .object(let schema)? = tool["inputSchema"]
    else {
      preconditionFailure("Native project-read tool definition is invalid")
    }
    let description: String
    if case .string(let value)? = tool["description"] {
      description = value
    } else {
      description = ""
    }
    return .object([
      "type": .string("function"),
      "name": .string(name),
      "description": .string(description),
      "parameters": .object(schema.mapValues(LocalAgentJSONValue.init(native:))),
    ])
  }

  private static func remoteCapabilityTool(_ value: NativeJSONValue) -> LocalAgentJSONValue {
    guard case .object(let tool) = value,
      case .string(let name)? = tool["name"],
      case .object(let schema)? = tool["inputSchema"]
    else {
      preconditionFailure("Native remote connection tool definition is invalid")
    }
    let description: String
    if case .string(let value)? = tool["description"] { description = value }
    else { description = "" }
    return .object([
      "type": .string("function"),
      "name": .string(remoteConnectionToolPrefix + name),
      "description": .string(description),
      "parameters": .object(schema.mapValues(LocalAgentJSONValue.init(native:))),
    ])
  }
}
