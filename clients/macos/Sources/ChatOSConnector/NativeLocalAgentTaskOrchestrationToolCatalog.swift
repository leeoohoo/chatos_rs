import ChatOSCore
import Foundation

extension NativeLocalAgentPlatformToolCatalog {
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
              .string("progress_summary"), .string("all_tasks"),
              .string("milestones"), .string("graph_summary"),
            ]),
            "default": .string("progress_summary"),
            "description": .string(
              "How Task progress is written back to Main Chat. progress_summary is the default and updates one short message per actual Run before delivering the final result; graph_summary is opt-in quiet mode; milestones emits explicitly marked milestones plus blockers and a final summary; all_tasks emits every Task lifecycle update."
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

}
