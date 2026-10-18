import ChatOSCore

extension NativeLocalAgentPlatformToolCatalog {
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
}
