import ChatOSCore
import Foundation

extension NativeLocalAgentPlatformToolCatalog {
  static let prerequisiteTaskIDs: LocalAgentJSONValue = .object([
    "type": .string("array"),
    "items": .object([
      "type": .string("string"), "minLength": .number(1),
    ]),
    "uniqueItems": .bool(true),
    "description": .string(
      "Existing Task ids that must complete successfully before this Task runs."
    ),
  ])

  static let taskSchedule: LocalAgentJSONValue = .object([
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

  static let builtinKindSelection: LocalAgentJSONValue = .object([
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

  static let unavailableExternalMCPSelection: LocalAgentJSONValue = .object([
    "type": .string("array"),
    "maxItems": .number(0),
    "items": .object(["type": .string("string")]),
  ])

  static var pluginHints: LocalAgentJSONValue { pluginHints([]) }

  static func pluginHints(
    _ choices: [NativeInstalledAgentPlugin]
  ) -> LocalAgentJSONValue {
    let choices = choices.sorted { $0.id < $1.id }
    var schema: [String: LocalAgentJSONValue] = [
      "type": .string("array"),
      "maxItems": .number(16),
      "uniqueItems": .bool(true),
      "description": .string(
        "Select the minimum installed Plugin components required by this specific Task. When the user explicitly names an installed Plugin or MCP and asks that it perform an operation, its matching component is required on every Task that performs that operation. Supporting components are additive and must never replace the named component. Use only exact plugin_key/component_key pairs from this request-scoped catalog. The client freezes and validates every selected component before execution."
      ),
      "items": .object([
        "oneOf": .array(choices.map { choice in
          .object([
            "title": .string(pluginChoiceTitle(choice)),
            "type": .string("object"),
            "properties": .object([
              "plugin_key": .object(["const": .string(choice.pluginKey)]),
              "component_key": .object(["const": .string(choice.componentKey)]),
              "reason": .object([
                "type": .string("string"), "maxLength": .number(1_000),
                "description": .string("Why this specific Task requires the Plugin component."),
              ]),
            ]),
            "required": .array([.string("plugin_key"), .string("component_key")]),
            "additionalProperties": .bool(false),
          ])
        }),
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

  static func taskIDTool(
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

  static func taskTool(
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
}
