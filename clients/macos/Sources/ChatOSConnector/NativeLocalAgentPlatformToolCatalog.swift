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
  public static let requirementSurveyCreateToolName = "requirement_survey_create"
  public static let requirementPlanSaveToolName = "requirement_plan_save"
  public static let requirementSurveyTools: [LocalAgentJSONValue] = [
    taskTool(
      name: requirementSurveyCreateToolName,
      description: "Collect material project goals, audience, constraints, preferences and acceptance decisions in the Requirement Survey center. Main Chat can ask before creating Tasks. Call alone, then wait for submitted answers; never invent answers or dispatch dependent work while waiting.",
      properties: [
        "title": .object(["type": .string("string")]),
        "description": .object(["type": .string("string")]),
        "questions": .object([
          "type": .string("array"), "minItems": .number(1), "maxItems": .number(50),
          "items": .object([
            "type": .string("object"), "additionalProperties": .bool(false),
            "properties": .object([
              "question_id": .object(["type": .string("string")]),
              "prompt": .object(["type": .string("string")]),
              "response_kind": .object(["type": .string("string"),
                "enum": .array(["text", "single_choice", "multiple_choice", "boolean"].map(LocalAgentJSONValue.string))]),
              "required": .object(["type": .string("boolean")]),
              "options": .object(["type": .string("array"), "maxItems": .number(50),
                "items": .object(["type": .string("string")])]),
            ]),
            "required": .array(["question_id", "prompt", "response_kind"].map(LocalAgentJSONValue.string)),
          ]),
        ]),
      ], required: ["title", "questions"]
    ),
    taskTool(
      name: requirementPlanSaveToolName,
      description: "Publish or revise a versioned proposal from submitted Requirement Survey answers. Owner, project and source Run are program-bound. Use expected_revision 0 for the initial draft, otherwise the current revision. Never confirm for the user; ask them to review and confirm the exact version in Requirement Survey. Saving does not start Tasks or authorize implementation.",
      properties: [
        "survey_id": .object(["type": .string("string"), "minLength": .number(1)]),
        "expected_revision": .object(["type": .string("integer"), "minimum": .number(0)]),
        "markdown": .object(["type": .string("string"), "minLength": .number(1), "maxLength": .number(65536)]),
      ], required: ["survey_id", "expected_revision", "markdown"]
    ),
  ]
  public static let listTasksToolName = "list_tasks"
  public static let getTaskToolName = "get_task"
  public static let createTaskToolName = "create_task"
  public static let createTasksToolName = "create_tasks_with_prerequisites"
  public static let cancelTaskToolName = "cancel_task"
  public static let waitForTaskCompletionToolName = "wait_for_task_completion"
  public static let getTaskDependencyGraphToolName = "get_task_dependency_graph"
  public static let resolveTaskFollowupToolName = "resolve_task_followup"
  public static let mainChatTaskToolNames = [
    listTasksToolName,
    getTaskToolName,
    createTaskToolName,
    createTasksToolName,
    cancelTaskToolName,
    waitForTaskCompletionToolName,
    getTaskDependencyGraphToolName,
    resolveTaskFollowupToolName,
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
        "Create one coherent durable local task. Use this only when splitting would not enable useful concurrency, specialization, ownership, or independent acceptance.",
      properties: [
        "title": .object(["type": .string("string"), "minLength": .number(1)]),
        "objective": .object(["type": .string("string"), "minLength": .number(1)]),
        "expected_outputs": taskContractList(
          "Concrete artifacts, decisions, reports, or verified changes this Task must return."
        ),
        "acceptance_criteria": taskContractList(
          "Observable, evidence-based conditions required before completion."
        ),
        "final_response_max_chars": taskFinalResponseLimit,
        "description": .object(["type": .string("string")]),
        "input_payload": .object([:]),
        "default_model_config_id": .object([
          "type": .string("string"), "minLength": .number(1),
        ]),
        "thinking_level": thinkingLevelOverride,
        "requires_execution": .object(["type": .string("boolean")]),
        "enabled_builtin_kinds": builtinKindSelection,
        "notepad_user_request_quote": notepadUserRequestQuote,
        "external_mcp_config_ids": unavailableExternalMCPSelection,
        "plugin_hints": pluginHints,
        "owned_paths": taskOwnedPaths,
        "prerequisite_task_ids": prerequisiteTaskIDs,
        "supersedes_task_ids": supersedesTaskIDs,
        "schedule": taskSchedule,
      ],
      required: [
        "title", "objective", "expected_outputs", "acceptance_criteria",
        "requires_execution", "enabled_builtin_kinds",
      ]
    ),
    .object([
      "type": .string("function"),
      "name": .string(createTasksToolName),
      "description": .string(
        "Create a durable local multi-Task graph for a compound outcome with distinct deliverables, specialist needs, safe parallel work, real dependencies, or independent verification."
      ),
      "parameters": .object([
        "type": .string("object"),
        "properties": .object([
          "supersedes_task_ids": supersedesTaskIDs,
          "graph_goal": .object([
            "type": .string("string"), "minLength": .number(1),
            "maxLength": .number(8_000),
          ]),
          "decomposition_rationale": .object([
            "type": .string("string"), "minLength": .number(1),
            "maxLength": .number(8_000),
          ]),
          "notification_policy": .object([
            "type": .string("string"),
            "enum": .array([
              .string("all_tasks"), .string("milestones"), .string("graph_summary"),
            ]),
            "default": .string("graph_summary"),
            "description": .string(
              "How Task progress is written back to Main Chat. graph_summary is the quiet default; milestones emits explicitly marked milestones plus blockers and a final summary; all_tasks emits every Task lifecycle update."
            ),
          ]),
          "tasks": .object([
            "type": .string("array"),
            "minItems": .number(2),
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
                "expected_outputs": taskContractList(
                  "Concrete artifacts, decisions, reports, or verified changes owned by this Task."
                ),
                "acceptance_criteria": taskContractList(
                  "Observable, evidence-based completion conditions for this Task."
                ),
                "final_response_max_chars": taskFinalResponseLimit,
                "description": .object(["type": .string("string")]),
                "input_payload": .object([:]),
                "default_model_config_id": .object([
                  "type": .string("string"), "minLength": .number(1),
                ]),
                "thinking_level": thinkingLevelOverride,
                "requires_execution": .object(["type": .string("boolean")]),
                "enabled_builtin_kinds": builtinKindSelection,
            "notepad_user_request_quote": notepadUserRequestQuote,
                "external_mcp_config_ids": unavailableExternalMCPSelection,
                "plugin_hints": pluginHints,
                "owned_paths": taskOwnedPaths,
                "notification_milestone": .object([
                  "type": .string("boolean"),
                  "default": .bool(false),
                  "description": .string(
                    "Under milestones policy, publish this Task's start and terminal result as an important user-visible milestone."
                  ),
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
                .string("expected_outputs"), .string("acceptance_criteria"),
                .string("requires_execution"),
                .string("enabled_builtin_kinds"),
              ]),
              "additionalProperties": .bool(false),
            ]),
          ])
        ]),
        "required": .array([
          .string("graph_goal"), .string("decomposition_rationale"), .string("tasks"),
        ]),
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
    taskTool(
      name: resolveTaskFollowupToolName,
      description: "Commit one decision for every current task_followup_review candidate.",
      properties: [
        "review_message_id": .object(["type": .string("string"), "minLength": .number(1)]),
        "decisions": .object([
          "type": .string("array"), "minItems": .number(1), "maxItems": .number(500),
          "items": .object([
            "type": .string("object"), "additionalProperties": .bool(false),
            "properties": .object([
              "task_id": .object(["type": .string("string"), "minLength": .number(1)]),
              "action": .object([
                "type": .string("string"),
                "enum": .array(["continue", "cancel", "clarify"].map(LocalAgentJSONValue.string)),
              ]),
              "reason": .object([
                "type": .string("string"), "minLength": .number(1), "maxLength": .number(1000),
              ]),
            ]),
            "required": .array(["task_id", "action", "reason"].map(LocalAgentJSONValue.string)),
          ]),
        ]),
      ],
      required: ["review_message_id", "decisions"]
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
    externalChoices: [NativeLocalAgentMCPChoice] = [],
    taskModelChoices: [NativeLocalAgentMCPChoice] = []
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
    let taskModelSchema = taskModelSelection(taskModelChoices)
    return (baseCapabilityTools + mainChatCompanionTools).map { tool in
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
        properties["default_model_config_id"] = taskModelSchema
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
        itemProperties["default_model_config_id"] = taskModelSchema
        items["properties"] = .object(itemProperties)
        tasks["items"] = .object(items)
        properties["tasks"] = .object(tasks)
        parameters["properties"] = .object(properties)
      }
      definition["parameters"] = .object(parameters)
      return .object(definition)
    }
  }

  private static func taskModelSelection(
    _ rawChoices: [NativeLocalAgentMCPChoice]
  ) -> LocalAgentJSONValue {
    let choices = Dictionary(uniqueKeysWithValues: rawChoices.map { ($0.value, $0) })
      .values.sorted { $0.value < $1.value }
    var schema: [String: LocalAgentJSONValue] = [
      "type": .string("string"),
      "minLength": .number(1),
      "description": .string(
        "Optional Task execution model. Only models enabled for Tasks are accepted. Omit it to reuse the Main Chat model when eligible, otherwise the first eligible Task model is used."
      ),
    ]
    if !choices.isEmpty {
      schema["enum"] = .array(choices.map { .string($0.value) })
      schema["oneOf"] = .array(choices.map {
        .object(["const": .string($0.value), "title": .string($0.title)])
      })
      schema["x-enum-labels"] = .array(choices.map { .string($0.title) })
    }
    return .object(schema)
  }

  private static let taskFinalResponseLimit: LocalAgentJSONValue = .object([
    "type": .string("integer"), "minimum": .number(1), "maximum": .number(32_000),
    "description": .string("Optional entire final user-facing response limit in Unicode scalar values, including whitespace, punctuation and Markdown; not a file-content limit."),
  ])

  private static let notepadUserRequestQuote: LocalAgentJSONValue = .object([
    "type": .string("string"), "minLength": .number(2), "maxLength": .number(500),
    "description": .string(
      "Required only when enabled_builtin_kinds contains Notepad and Notepad was not explicitly selected in the UI. Copy an exact phrase from the current user message that asks to record/save something in Notepad or identifies an existing note as input. Do not paraphrase and do not use generic task text as evidence."
    ),
  ])

  static func upgradingTaskAcceptanceSchema(_ tool: LocalAgentJSONValue) -> LocalAgentJSONValue {
    guard case .object(var definition) = tool,
          case .string(let name)? = definition["name"],
          name == createTaskToolName || name == createTasksToolName,
          case .object(var parameters)? = definition["parameters"],
          case .object(var properties)? = parameters["properties"] else { return tool }
    if name == createTaskToolName {
      properties["final_response_max_chars"] = taskFinalResponseLimit
      properties["notepad_user_request_quote"] = notepadUserRequestQuote
    } else if case .object(var tasks)? = properties["tasks"],
              case .object(var items)? = tasks["items"],
              case .object(var itemProperties)? = items["properties"] {
      itemProperties["final_response_max_chars"] = taskFinalResponseLimit
      itemProperties["notepad_user_request_quote"] = notepadUserRequestQuote
      items["properties"] = .object(itemProperties)
      tasks["items"] = .object(items)
      properties["tasks"] = .object(tasks)
    }
    parameters["properties"] = .object(properties)
    definition["parameters"] = .object(parameters)
    return .object(definition)
  }

  private static var taskOwnedPaths: LocalAgentJSONValue {
    .object([
      "type": .string("array"), "maxItems": .number(200),
      "items": .object([
        "type": .string("string"), "minLength": .number(1), "maxLength": .number(4096),
      ]),
      "uniqueItems": .bool(true),
      "description": .string(
        "Normalized project-relative files or directories owned by this Task when CodeMaintainerWrite is enabled. Independent write Tasks must use disjoint paths. An omitted or empty list conservatively claims the whole project; add a prerequisite when writers must touch the same path."
      ),
    ])
  }

  private static func taskContractList(_ description: String) -> LocalAgentJSONValue {
    .object([
      "type": .string("array"),
      "minItems": .number(1),
      "maxItems": .number(50),
      "items": .object([
        "type": .string("string"), "minLength": .number(1),
        "maxLength": .number(4_000),
      ]),
      "description": .string(description),
    ])
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
      "description": .string(
        "Select only capabilities explicitly required by the user's requested deliverable or named input. Never add a capability because it might be useful for scratch work or generic persistence. Select Notepad only when the user explicitly requests a Notepad record or identifies an existing note as input."
      ),
    ]
    if choices.isEmpty {
      schema["maxItems"] = .number(0)
      schema["description"] = .string(emptyDescription)
    }
    return .object(schema)
  }

  private static let supersedesTaskIDs: LocalAgentJSONValue = .object([
    "type": .string("array"), "maxItems": .number(50), "uniqueItems": .bool(true),
    "items": .object(["type": .string("string"), "minLength": .number(1)]),
    "description": .string(
      "Ids of affected old tasks already confirmed cancelled with cancel_task. Creates corrected work instead of reusing the old source graph; all ids must belong to this conversation. Omit for ordinary creation."
    ),
  ])

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

  static let attachmentReadTool: LocalAgentJSONValue = .object([
    "type": .string("function"),
    "name": .string(attachmentReadToolName),
    "description": .string(
      "Read an authorized conversation attachment. Use delivery=model to see the complete verified image directly, without a project, Plugin, or Task. Task delivery=workspace imports the complete verified file and also supplies supported images to the model; a file path alone is not image input. Use delivery=content for bounded text reads, not image base64 pagination. Treat authorized_local_ref as opaque."
    ),
    "parameters": .object([
      "type": .string("object"),
      "properties": .object([
        "delivery": .object([
          "type": .string("string"),
          "enum": .array([.string("workspace"), .string("content"), .string("model")]),
          "default": .string("workspace"),
          "description": .string("model supplies a complete image as multimodal input; workspace imports the entire file; both ignore offset/limit. content reads a bounded text segment."),
        ]),
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
          "description": .string("Upper bound for content delivery; inline chunks are capped at 2048 bytes to stay below model transport limits. Follow next_offset to read more."),
        ]),
      ]),
      "required": .array([.string("authorized_local_ref")]),
      "additionalProperties": .bool(false),
    ]),
  ])

  // Attachment access is a read-only conversation capability, not a ninth Task
  // orchestration tool. Main Chat must not create a workspace merely to see an image.
  private static var mainChatAttachmentReadTool: LocalAgentJSONValue {
    guard case var .object(tool) = attachmentReadTool,
          case var .object(parameters)? = tool["parameters"],
          case var .object(properties)? = parameters["properties"],
          case var .object(delivery)? = properties["delivery"] else { return attachmentReadTool }
    delivery["enum"] = .array([.string("model"), .string("content")])
    delivery["default"] = .string("model")
    properties["delivery"] = .object(delivery)
    parameters["properties"] = .object(properties)
    tool["parameters"] = .object(parameters)
    return .object(tool)
  }

  static var mainChatCompanionTools: [LocalAgentJSONValue] {
    requirementSurveyTools + [mainChatAttachmentReadTool]
  }

  static func upgradingAttachmentReadSchema(_ tools: [LocalAgentJSONValue]) -> [LocalAgentJSONValue] {
    var found = false
    var result = tools.compactMap { tool -> LocalAgentJSONValue? in
      guard case let .object(value) = tool, value["name"] == .string(attachmentReadToolName) else { return tool }
      guard !found else { return nil }
      found = true
      return attachmentReadTool
    }
    if !found { result.append(attachmentReadTool) }
    return result
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
