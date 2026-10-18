import ChatOSCore

extension NativeLocalAgentPlatformToolCatalog {
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
}
