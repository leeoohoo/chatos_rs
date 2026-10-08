import ChatOSCore
import Foundation

public struct NativeLocalAgentMCPChoice: Sendable, Equatable {
  public let value: String
  public let title: String

  public init(value: String, title: String) {
    self.value = value
    self.title = title
  }
}

public enum NativeLocalAgentPlatformToolCatalog {
  public static let attachmentReadToolName = "local_attachment_read"
  public static let listTasksToolName = "list_tasks"
  public static let getTaskToolName = "get_task"
  public static let createTaskToolName = "create_task"
  public static let createTasksToolName = "create_tasks_with_prerequisites"
  public static let cancelTaskToolName = "cancel_task"
  public static let waitForTaskCompletionToolName = "wait_for_task_completion"
  public static let getTaskDependencyGraphToolName = "get_task_dependency_graph"
  public static let mainChatTaskToolNames = [
    listTasksToolName,
    getTaskToolName,
    createTaskToolName,
    createTasksToolName,
    cancelTaskToolName,
    waitForTaskCompletionToolName,
    getTaskDependencyGraphToolName,
  ]
  private static let projectReadOnlyToolNames = [
    "read_file_raw", "read_file_range", "list_dir", "search_text", "read_file",
    "search_files",
  ]
  private static let terminalReadOnlyToolNames = [
    "process_poll", "process_log", "process_wait",
  ]
  static let remoteConnectionToolPrefix = "remote_connection_controller_"
  static let taskExecutionRemoteReadOnlyToolNames: Set<String> = Set(
    ["test_connection", "list_directory", "read_file", "download_file"]
      .map { remoteConnectionToolPrefix + $0 }
  )
  static let taskExecutionRemoteToolNames: Set<String> = Set(
    NativeMCPRemoteConnectionController.toolNames.map { remoteConnectionToolPrefix + $0 }
  )
  private static let pluginReadOnlyToolNames =
    NativeAgentCapabilityBrokerToolCatalog.readOnlyToolNames.sorted()
  static let taskExecutionTerminalToolNames: Set<String> = [
    "execute_command", "process_poll", "process_log", "process_wait", "process_write",
    "process_kill",
  ]
  public static let readOnlyToolNames =
    [attachmentReadToolName]
    + projectReadOnlyToolNames + terminalReadOnlyToolNames
    + taskExecutionRemoteReadOnlyToolNames.sorted()
    + pluginReadOnlyToolNames
  public static let approvalExemptToolNames = [
    "open_edit_session", "stage_edit_batch", "abort_edit_session",
    NativeAgentCapabilityBrokerToolCatalog.invokeToolName,
  ]

  private static let baseCapabilityTools: [LocalAgentJSONValue] = [
    taskTool(
      name: listTasksToolName,
      description:
        "List durable local tasks created from the current conversation/project. Use keyword when the user refers to earlier work.",
      properties: [
        "status": .object([
          "type": .string("string"),
          "enum": .array(
            [
              "draft", "ready", "queued", "running", "succeeded", "failed",
              "blocked", "cancelled", "archived",
            ].map(LocalAgentJSONValue.string)),
        ]),
        "keyword": .object(["type": .string("string"), "maxLength": .number(500)]),
        "tag": .object(["type": .string("string"), "maxLength": .number(256)]),
        "scheduled_only": .object(["type": .string("boolean")]),
        "parent_task_id": .object([
          "type": .string("string"), "maxLength": .number(256),
        ]),
        "source_run_id": .object([
          "type": .string("string"), "maxLength": .number(256),
        ]),
        "limit": .object([
          "type": .string("integer"), "minimum": .number(1),
          "maximum": .number(500), "default": .number(50),
        ]),
        "offset": .object([
          "type": .string("integer"), "minimum": .number(0),
          "maximum": .number(100_000), "default": .number(0),
        ]),
      ]
    ),
    taskIDTool(
      name: getTaskToolName,
      description: "Get one durable local task created from the current conversation/project."
    ),
    taskTool(
      name: createTaskToolName,
      description:
        "Create one durable local task for the current conversation/project. Use this whenever answering requires inspecting project files, using execution tools, or doing tracked work; never ask the user to re-upload an already bound project.",
      properties: [
        "title": .object(["type": .string("string"), "minLength": .number(1)]),
        "objective": .object(["type": .string("string"), "minLength": .number(1)]),
        "description": .object(["type": .string("string")]),
        "input_payload": .object([:]),
        "default_model_config_id": .object([
          "type": .string("string"), "minLength": .number(1),
        ]),
        "thinking_level": thinkingLevelOverride,
        "requires_execution": .object(["type": .string("boolean")]),
        "enabled_builtin_kinds": builtinKindSelection,
        "external_mcp_config_ids": unavailableExternalMCPSelection,
        "plugin_hints": pluginHints,
        "prerequisite_task_ids": prerequisiteTaskIDs,
        "schedule": taskSchedule,
      ],
      required: ["title", "objective", "requires_execution", "enabled_builtin_kinds"]
    ),
    .object([
      "type": .string("function"),
      "name": .string(createTasksToolName),
      "description": .string(
        "Create a durable local task graph for the current conversation/project. Use investigation, implementation and review stages when prerequisites are needed instead of asking the user to provide the bound project again."
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
                "client_ref": .object([
                  "type": .string("string"), "minLength": .number(1),
                ]),
                "title": .object([
                  "type": .string("string"), "minLength": .number(1),
                ]),
                "objective": .object([
                  "type": .string("string"), "minLength": .number(1),
                ]),
                "description": .object(["type": .string("string")]),
                "input_payload": .object([:]),
                "default_model_config_id": .object([
                  "type": .string("string"), "minLength": .number(1),
                ]),
                "thinking_level": thinkingLevelOverride,
                "requires_execution": .object(["type": .string("boolean")]),
                "enabled_builtin_kinds": builtinKindSelection,
                "external_mcp_config_ids": unavailableExternalMCPSelection,
                "plugin_hints": pluginHints,
                "owned_paths": .object([
                  "type": .string("array"),
                  "maxItems": .number(200),
                  "items": .object([
                    "type": .string("string"), "minLength": .number(1),
                  ]),
                  "uniqueItems": .bool(true),
                ]),
                "prerequisite_refs": .object([
                  "type": .string("array"),
                  "items": .object([
                    "type": .string("string"), "minLength": .number(1),
                  ]),
                  "uniqueItems": .bool(true),
                ]),
                "context_refs": .object([
                  "type": .string("array"),
                  "items": .object([
                    "type": .string("string"), "minLength": .number(1),
                  ]),
                  "uniqueItems": .bool(true),
                  "description": .string(
                    "Non-blocking context relationships used for explanation and graph display; they never delay scheduling."
                  ),
                ]),
                "prerequisite_task_ids": prerequisiteTaskIDs,
                "schedule": taskSchedule,
              ]),
              "required": .array([
                .string("client_ref"), .string("title"), .string("objective"),
                .string("requires_execution"),
                .string("enabled_builtin_kinds"),
              ]),
              "additionalProperties": .bool(false),
            ]),
          ])
        ]),
        "required": .array([.string("tasks")]),
        "additionalProperties": .bool(false),
      ]),
    ]),
    taskTool(
      name: cancelTaskToolName,
      description:
        "Cancel a pending or running local task from the current conversation/project because it conflicts with the user's latest intent.",
      properties: [
        "task_id": .object(["type": .string("string"), "minLength": .number(1)]),
        "reason": .object([
          "type": .string("string"), "minLength": .number(1),
          "maxLength": .number(1_000),
        ]),
        "replacement_task_ids": .object([
          "type": .string("array"),
          "items": .object([
            "type": .string("string"), "minLength": .number(1),
          ]),
          "uniqueItems": .bool(true),
          "description": .string(
            "New Task ids that supersede this Task; this internal replacement cancellation is not shown as a user-facing callback."
          ),
        ]),
      ],
      required: ["task_id", "reason"]
    ),
    taskTool(
      name: waitForTaskCompletionToolName,
      description:
        "Use exactly once after tasks have been created or adjusted. This is a background handoff signal, not polling; the final result is written back to this conversation."
    ),
    taskIDTool(
      name: getTaskDependencyGraphToolName,
      description:
        "Get the complete dependency graph containing one task from the current conversation/project."
    ),
  ]

  public static var capabilityTools: [LocalAgentJSONValue] {
    capabilityTools(pluginChoices: [], builtinChoices: [], externalChoices: [])
  }

  private static let thinkingLevelOverride: LocalAgentJSONValue = .object([
    "type": .string("string"),
    "enum": .array(
      ["none", "auto", "minimal", "low", "medium", "high", "xhigh", "max"]
        .map(LocalAgentJSONValue.string)
    ),
    "description": .string(
      "Optional reasoning level override for this Task. Omit it to use the selected model configuration's default Thinking level."
    ),
  ])

  public static func capabilityTools(
    pluginChoices: [NativeInstalledAgentPlugin],
    builtinChoices: [NativeLocalAgentMCPChoice] = [],
    externalChoices: [NativeLocalAgentMCPChoice] = []
  ) -> [LocalAgentJSONValue] {
    let pluginSchema = pluginHints(pluginChoices)
    let builtinSchema = mcpSelection(
      builtinChoices,
      emptyDescription: "No builtin MCP capability is selectable for this Agent binding. Send an empty enabled_builtin_kinds array."
    )
    let externalSchema = mcpSelection(
      externalChoices,
      emptyDescription: "No external MCP configuration is selectable for this Agent binding. Send an empty external_mcp_config_ids array."
    )
    return baseCapabilityTools.map { tool in
      guard case .object(var definition) = tool,
        case .string(let name)? = definition["name"],
        case .object(var parameters)? = definition["parameters"]
      else { return tool }
      if name == createTaskToolName,
        case .object(var properties)? = parameters["properties"]
      {
        properties["enabled_builtin_kinds"] = builtinSchema
        properties["external_mcp_config_ids"] = externalSchema
        properties["plugin_hints"] = pluginSchema
        parameters["properties"] = .object(properties)
      } else if name == createTasksToolName,
        case .object(var properties)? = parameters["properties"],
        case .object(var tasks)? = properties["tasks"],
        case .object(var items)? = tasks["items"],
        case .object(var itemProperties)? = items["properties"]
      {
        itemProperties["enabled_builtin_kinds"] = builtinSchema
        itemProperties["external_mcp_config_ids"] = externalSchema
        itemProperties["plugin_hints"] = pluginSchema
        items["properties"] = .object(itemProperties)
        tasks["items"] = .object(items)
        properties["tasks"] = .object(tasks)
        parameters["properties"] = .object(properties)
      }
      definition["parameters"] = .object(parameters)
      return .object(definition)
    }
  }

  private static func mcpSelection(
    _ rawChoices: [NativeLocalAgentMCPChoice],
    emptyDescription: String
  ) -> LocalAgentJSONValue {
    let choices = Dictionary(uniqueKeysWithValues: rawChoices.map { ($0.value, $0) })
      .values.sorted { $0.value < $1.value }
    var item: [String: LocalAgentJSONValue] = [
      "type": .string("string"), "minLength": .number(1),
    ]
    if !choices.isEmpty {
      item["enum"] = .array(choices.map { .string($0.value) })
      item["oneOf"] = .array(choices.map {
        .object(["const": .string($0.value), "title": .string($0.title)])
      })
      item["x-enum-labels"] = .array(choices.map { .string($0.title) })
    }
    var schema: [String: LocalAgentJSONValue] = [
      "type": .string("array"),
      "items": .object(item),
      "uniqueItems": .bool(true),
    ]
    if choices.isEmpty {
      schema["maxItems"] = .number(0)
      schema["description"] = .string(emptyDescription)
    }
    return .object(schema)
  }

  private static let prerequisiteTaskIDs: LocalAgentJSONValue = .object([
    "type": .string("array"),
    "items": .object([
      "type": .string("string"), "minLength": .number(1),
    ]),
    "uniqueItems": .bool(true),
    "description": .string(
      "Existing local Task ids that must complete successfully before this Task runs."
    ),
  ])

  private static let taskSchedule: LocalAgentJSONValue = .object([
    "type": .string("object"),
    "properties": .object([
      "mode": .object([
        "type": .string("string"),
        "enum": .array(
          ["manual", "once", "interval", "contact_async"].map(
            LocalAgentJSONValue.string
          )),
      ]),
      "run_at": .object([
        "type": .string("string"),
        "description": .string("Optional RFC 3339 time at which this Task may start."),
      ]),
      "interval_seconds": .object([
        "type": .string("integer"), "minimum": .number(1),
      ]),
    ]),
    "additionalProperties": .bool(false),
  ])

  private static let builtinKindSelection: LocalAgentJSONValue = .object([
    "type": .string("array"),
    "items": .object([
      "type": .string("string"),
      "enum": .array(
        [
          "CodeMaintainerRead", "CodeMaintainerWrite", "TerminalController",
          "RequirementSurveyRead", "RequirementSurveyWrite", "Notepad",
        ].map(LocalAgentJSONValue.string)),
    ]),
    "uniqueItems": .bool(true),
  ])

  private static let unavailableExternalMCPSelection: LocalAgentJSONValue = .object([
    "type": .string("array"),
    "maxItems": .number(0),
    "items": .object(["type": .string("string")]),
  ])

  private static var pluginHints: LocalAgentJSONValue { pluginHints([]) }

  private static func pluginHints(
    _ choices: [NativeInstalledAgentPlugin]
  ) -> LocalAgentJSONValue {
    let choices = Dictionary(uniqueKeysWithValues: choices.map { ($0.pluginKey, $0) })
      .values.sorted { $0.pluginKey < $1.pluginKey }
    var pluginKey: [String: LocalAgentJSONValue] = [
      "type": .string("string"), "minLength": .number(1),
    ]
    if !choices.isEmpty {
      pluginKey["enum"] = .array(choices.map { .string($0.pluginKey) })
      pluginKey["oneOf"] = .array(choices.map {
        .object([
          "const": .string($0.pluginKey),
          "title": .string(pluginChoiceTitle($0)),
        ])
      })
      pluginKey["x-enum-labels"] = .array(choices.map {
        .string(pluginChoiceTitle($0))
      })
    }
    var schema: [String: LocalAgentJSONValue] = [
      "type": .string("array"),
      "maxItems": .number(16),
      "uniqueItems": .bool(true),
      "description": .string(
        "Suggest only the minimum installed Plugins required by this specific Task. Use only plugin_key values from this request-scoped catalog. Route by the actual interaction surface: use Computer Use for native desktop applications and operating-system UI; use Browser CDP only for websites in managed Chromium or an explicitly connected Chrome session. Do not select both merely as a fallback. The client freezes and validates the selected Plugin keys before execution."
      ),
      "items": .object([
        "type": .string("object"),
        "properties": .object([
          "plugin_key": .object(pluginKey),
          "reason": .object([
            "type": .string("string"), "maxLength": .number(1_000),
            "description": .string("Why this specific Task requires the Plugin."),
          ]),
        ]),
        "required": .array([.string("plugin_key")]),
        "additionalProperties": .bool(false),
      ]),
    ]
    if choices.isEmpty {
      schema["maxItems"] = .number(0)
      schema["description"] = .string(
        "No installed Plugin is selectable for this request. Send an empty plugin_hints array."
      )
    }
    return .object(schema)
  }

  private static func pluginChoiceTitle(
    _ plugin: NativeInstalledAgentPlugin
  ) -> String {
    let key = plugin.pluginKey.trimmingCharacters(in: .whitespacesAndNewlines)
      .lowercased()
    let description: String
    if key.contains("computer-use") {
      description = plugin.description
        + " Use this for native desktop applications and operating-system UI, including Feishu/Lark, WeChat, DingTalk, Finder and other installed apps. Prefer it over browser automation whenever the target is a desktop app."
    } else if key.contains("browser-cdp") {
      description = plugin.description
        + " Use this only for websites in managed Chromium or an explicitly connected Chrome session. Do not select it for native desktop applications such as Feishu/Lark, WeChat or DingTalk."
    } else {
      description = plugin.description
    }
    return "\(plugin.displayName) — \(description)"
  }

  private static func taskIDTool(
    name: String,
    description: String
  ) -> LocalAgentJSONValue {
    taskTool(
      name: name,
      description: description,
      properties: [
        "task_id": .object(["type": .string("string"), "minLength": .number(1)])
      ],
      required: ["task_id"]
    )
  }

  private static func taskTool(
    name: String,
    description: String,
    properties: [String: LocalAgentJSONValue] = [:],
    required: [String] = []
  ) -> LocalAgentJSONValue {
    var parameters: [String: LocalAgentJSONValue] = [
      "type": .string("object"),
      "properties": .object(properties),
      "additionalProperties": .bool(false),
    ]
    if !required.isEmpty {
      parameters["required"] = .array(required.map(LocalAgentJSONValue.string))
    }
    return .object([
      "type": .string("function"),
      "name": .string(name),
      "description": .string(description),
      "parameters": .object(parameters),
    ])
  }

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

  private static let attachmentReadTool: LocalAgentJSONValue = .object([
    "type": .string("function"),
    "name": .string(attachmentReadToolName),
    "description": .string(
      "Read a bounded segment of an attachment authorized by the source conversation. Treat authorized_local_ref as opaque."
    ),
    "parameters": .object([
      "type": .string("object"),
      "properties": .object([
        "authorized_local_ref": .object([
          "type": .string("string"), "minLength": .number(1),
          "maxLength": .number(160),
        ]),
        "offset": .object([
          "type": .string("integer"), "minimum": .number(0),
          "default": .number(0),
        ]),
        "limit": .object([
          "type": .string("integer"), "minimum": .number(1),
          "maximum": .number(65_536), "default": .number(16_384),
        ]),
      ]),
      "required": .array([.string("authorized_local_ref")]),
      "additionalProperties": .bool(false),
    ]),
  ])

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
