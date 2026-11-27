import ChatOSCore

extension NativeLocalAgentPlatformToolCatalog {
  public static let requirementSurveyCreateToolName = "requirement_survey_create"
  public static let requirementPlanSaveToolName = "requirement_plan_save"
  public static let requirementSurveyTools: [LocalAgentJSONValue] = [
    taskTool(
      name: requirementSurveyCreateToolName,
      description: "Collect material project goals, audience, constraints, preferences and acceptance decisions in the Requirement Survey center. Project Main Chat, an authorized Agent team survey Todo, or a blocked Task can ask. Call alone, then wait for submitted answers; never invent answers or dispatch dependent work while waiting.",
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
      description: "Publish or revise a versioned proposal from a Plugin-authorized planning Task, submitted Requirement Survey answers, or the program-bound current plan. Every internal ID, owner, project and source Run is program-bound. Submit only expected_revision and markdown. Use expected_revision 0 for the initial draft, otherwise the current revision. Never confirm for the user; ask them to review and confirm the exact version before execution.",
      properties: [
        "expected_revision": .object(["type": .string("integer"), "minimum": .number(0)]),
        "markdown": .object(["type": .string("string"), "minLength": .number(1), "maxLength": .number(65536)]),
      ], required: ["expected_revision", "markdown"]
    ),
  ]
}
