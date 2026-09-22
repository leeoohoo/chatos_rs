import ChatOSCore
import Foundation

/// Project-bound Requirement Survey MCP. The caller never supplies an account, project, room or
/// team identifier; those values are fixed by the task runtime before these tools are exposed.
struct NativeMCPRequirementSurveyTools: Sendable {
    let store: SQLiteAgentGroupChatStore
    let ownerUserID: String
    let projectID: String
    let creatorAgentID: String
    let sourceDeliveryID: String
    let now: @Sendable () -> Int64

    static let readToolNames: Set<String> = [
        "skill_activate",
        "skill_list_resources",
        "skill_read_resource",
        "requirement_survey_list",
        "requirement_survey_get",
        "requirement_survey_project_tasks",
    ]

    static let writeToolNames: Set<String> = [
        "requirement_survey_create",
        "requirement_survey_resolve",
    ]

    static var readToolDefinitions: [NativeJSONValue] {
        [
            definition(
                name: "skill_activate",
                description: "激活当前需求调研 Skill Catalog 中的一个不可变 Skill。使用系统目录给出的 skill_ref；先激活 Router，再按其路由激活一个专业 Skill。",
                properties: [
                    "skill_ref": stringSchema(maximum: 80),
                ],
                required: ["skill_ref"]
            ),
            definition(
                name: "skill_list_resources",
                description: "列出一个需求调研 Skill 声明的按需资源。skill_ref 必须来自当前 Skill Catalog。",
                properties: ["skill_ref": stringSchema(maximum: 80)],
                required: ["skill_ref"]
            ),
            definition(
                name: "skill_read_resource",
                description: "按需读取需求调研 Skill 的一个文本资源，例如具体工具参数或结果示例。",
                properties: [
                    "skill_ref": stringSchema(maximum: 80),
                    "relative_path": stringSchema(maximum: 1_000),
                    "offset": .object([
                        "type": .string("integer"), "minimum": .number(0),
                    ]),
                    "max_chars": .object([
                        "type": .string("integer"), "minimum": .number(1),
                        "maximum": .number(64_000),
                    ]),
                ],
                required: ["skill_ref", "relative_path"]
            ),
            definition(
                name: "requirement_survey_list",
                description: "列出当前任务绑定项目的需求调研摘要。创建前用 pending 去重；处理 Human 提交时用 submitted 定位。项目 ID 由程序透传。",
                properties: [
                    "status": .object([
                        "type": .string("string"),
                        "enum": .array([.string("pending"), .string("submitted")]),
                    ]),
                ]
            ),
            definition(
                name: "requirement_survey_get",
                description: "读取当前项目一张调研单的完整题目、Human 选项答案、备注、解决方案和执行计划。survey_id 必须来自本轮 list 结果。",
                properties: [
                    "survey_id": .object([
                        "type": .string("string"), "minLength": .number(1),
                    ]),
                ],
                required: ["survey_id"]
            ),
            definition(
                name: "requirement_survey_project_tasks",
                description: "读取当前项目团队任务板，包括任务目标、状态、负责人、执行能力、阻塞和结果。项目与团队由程序解析，不接受 Team/Room ID。"
            ),
        ]
    }

    static var writeToolDefinitions: [NativeJSONValue] {
        [
            definition(
                name: "requirement_survey_create",
                description: "在当前任务绑定项目创建需求调研单。调用前必须先 list/get 排除重复。只能创建单选或多选题；客户端页面会统一提供备注框。",
                properties: [
                    "request_key": stringSchema(maximum: 512),
                    "title": stringSchema(maximum: 240),
                    "purpose": stringSchema(maximum: 4_000),
                    "questions": .object([
                        "type": .string("array"),
                        "minItems": .number(1),
                        "maxItems": .number(12),
                        "items": .object([
                            "type": .string("object"),
                            "properties": .object([
                                "key": stringSchema(maximum: 120),
                                "prompt": stringSchema(maximum: 1_000),
                                "kind": .object([
                                    "type": .string("string"),
                                    "enum": .array([
                                        .string("single_choice"),
                                        .string("multiple_choice"),
                                    ]),
                                ]),
                                "required": .object(["type": .string("boolean")]),
                                "options": .object([
                                    "type": .string("array"),
                                    "minItems": .number(2),
                                    "maxItems": .number(12),
                                    "items": .object([
                                        "type": .string("object"),
                                        "properties": .object([
                                            "key": stringSchema(maximum: 120),
                                            "label": stringSchema(maximum: 500),
                                        ]),
                                        "required": .array([.string("key"), .string("label")]),
                                        "additionalProperties": .bool(false),
                                    ]),
                                ]),
                            ]),
                            "required": .array([
                                .string("key"), .string("prompt"), .string("kind"),
                                .string("options"),
                            ]),
                            "additionalProperties": .bool(false),
                        ]),
                    ]),
                ],
                required: ["request_key", "title", "purpose", "questions"]
            ),
            definition(
                name: "requirement_survey_resolve",
                description: "为已由 Human 提交的当前项目调研写入解决方案和结构化执行计划。调用前必须先 list/get；pending 调研不能生成方案。",
                properties: [
                    "survey_id": stringSchema(maximum: 512),
                    "summary": stringSchema(maximum: 4_000),
                    "solution_markdown": stringSchema(maximum: 128_000),
                    "execution_steps": .object([
                        "type": .string("array"),
                        "minItems": .number(1),
                        "maxItems": .number(50),
                        "items": .object([
                            "type": .string("object"),
                            "properties": .object([
                                "key": stringSchema(maximum: 120),
                                "title": stringSchema(maximum: 500),
                                "detail": stringSchema(maximum: 8_000),
                                "owner": optionalStringSchema(maximum: 500),
                                "deliverable": optionalStringSchema(maximum: 4_000),
                                "acceptance_criteria": optionalStringSchema(maximum: 4_000),
                            ]),
                            "required": .array([
                                .string("key"), .string("title"), .string("detail"),
                            ]),
                            "additionalProperties": .bool(false),
                        ]),
                    ]),
                    "risks_and_open_questions": optionalStringSchema(maximum: 32_000),
                    "related_materials": optionalStringSchema(maximum: 32_000),
                ],
                required: ["survey_id", "summary", "solution_markdown", "execution_steps"]
            ),
        ]
    }

    func call(name: String, arguments: [String: NativeJSONValue]) async throws -> NativeJSONValue {
        switch name {
        case "skill_activate":
            let activation = try LocalAgentProgressiveSkillCatalog
                .activateRequirementSurveySkill(
                    skillRef: requiredString(arguments, "skill_ref")
                )
            return .object([
                "activated": .bool(true),
                "skill_ref": .string(activation.skill.skillRef),
                "name": .string(activation.skill.name),
                "role": .string(activation.skill.role),
                "instructions": .string(activation.instructions),
                "instructions_sha256": .string(activation.instructionsSHA256),
                "resources": .array(activation.resources.map(resource)),
            ])

        case "skill_list_resources":
            let skillRef = try requiredString(arguments, "skill_ref")
            return .object([
                "skill_ref": .string(skillRef),
                "resources": .array(try LocalAgentProgressiveSkillCatalog
                    .requirementSurveyResources(skillRef: skillRef).map(resource)),
            ])

        case "skill_read_resource":
            let skillRef = try requiredString(arguments, "skill_ref")
            let relativePath = try ProgressiveSkillFileLoader.normalizedRelativePath(
                requiredString(arguments, "relative_path")
            )
            let page = try LocalAgentProgressiveSkillCatalog.readRequirementSurveyResource(
                skillRef: skillRef,
                relativePath: relativePath,
                offset: Int(arguments["offset"]?.jsonNumber ?? 0),
                maximumCharacters: Int(arguments["max_chars"]?.jsonNumber ?? 32_000)
            )
            let descriptor = try LocalAgentProgressiveSkillCatalog
                .requirementSurveyResources(skillRef: skillRef)
                .first(where: { $0.relativePath == relativePath })
            return .object([
                "skill_ref": .string(skillRef),
                "relative_path": .string(relativePath),
                "sha256": descriptor.map { .string($0.sha256) } ?? .null,
                "content": .string(page.content),
                "offset": .number(Double(page.offset)),
                "next_offset": page.nextOffset.map { .number(Double($0)) } ?? .null,
                "truncated": .bool(page.truncated),
            ])

        case "requirement_survey_list":
            let status: LocalAgentRequirementSurveyStatus?
            if let rawStatus = arguments["status"]?.jsonString {
                guard let parsed = LocalAgentRequirementSurveyStatus(rawValue: rawStatus) else {
                    throw ToolError.invalid("status 必须是 pending 或 submitted")
                }
                status = parsed
            } else {
                status = nil
            }
            let surveys = try await store.listRequirementSurveys(
                ownerUserID: ownerUserID,
                projectID: projectID,
                status: status
            )
            return .object([
                "project_bound": .bool(true),
                "surveys": .array(surveys.map(summary)),
            ])

        case "requirement_survey_get":
            let surveyID = try requiredString(arguments, "survey_id")
            guard let survey = try await store.requirementSurvey(
                ownerUserID: ownerUserID,
                projectID: projectID,
                surveyID: surveyID
            ) else { throw ToolError.notFound }
            return detail(survey)

        case "requirement_survey_project_tasks":
            let rooms = try await store.listRooms(
                ownerUserID: ownerUserID,
                includeArchived: false
            ).filter {
                $0.projectID == projectID && $0.conversationKind == .projectTeam
            }
            var teams: [NativeJSONValue] = []
            for room in rooms {
                let todos = try await store.listTeamTodos(
                    ownerUserID: ownerUserID,
                    teamRoomID: room.id,
                    includeTerminal: true
                )
                teams.append(.object([
                    "team_name": .string(room.draft.name),
                    "team_goal": .string(room.draft.goal),
                    "tasks": .array(todos.map(task)),
                ]))
            }
            return .object(["project_bound": .bool(true), "teams": .array(teams)])

        case "requirement_survey_create":
            let questions = try requiredArray(arguments, "questions").map { value in
                guard let object = value.jsonObject,
                      let kind = LocalAgentRequirementSurveyQuestionKind(
                        rawValue: try requiredString(object, "kind")
                      ) else { throw ToolError.invalid("questions.kind 无效") }
                let options = try requiredArray(object, "options").map { optionValue in
                    guard let option = optionValue.jsonObject else {
                        throw ToolError.invalid("questions.options 必须是对象数组")
                    }
                    return LocalAgentRequirementSurveyOption(
                        id: try requiredString(option, "key"),
                        label: try requiredString(option, "label")
                    )
                }
                return LocalAgentRequirementSurveyQuestion(
                    id: try requiredString(object, "key"),
                    prompt: try requiredString(object, "prompt"),
                    kind: kind,
                    options: options,
                    isRequired: object["required"]?.jsonBool ?? true
                )
            }
            let survey = try await store.createRequirementSurvey(
                ownerUserID: ownerUserID,
                projectID: projectID,
                creatorAgentID: creatorAgentID,
                sourceDeliveryID: sourceDeliveryID,
                requestKey: try requiredString(arguments, "request_key"),
                draft: .init(
                    title: try requiredString(arguments, "title"),
                    purpose: try requiredString(arguments, "purpose"),
                    questions: questions
                ),
                nowUnixMs: now()
            )
            return detail(survey)

        case "requirement_survey_resolve":
            let steps = try requiredArray(arguments, "execution_steps").map { value in
                guard let object = value.jsonObject else {
                    throw ToolError.invalid("execution_steps 必须是对象数组")
                }
                return LocalAgentRequirementSurveyExecutionStep(
                    id: try requiredString(object, "key"),
                    title: try requiredString(object, "title"),
                    detail: try requiredString(object, "detail"),
                    owner: object["owner"]?.jsonString ?? "",
                    deliverable: object["deliverable"]?.jsonString ?? "",
                    acceptanceCriteria: object["acceptance_criteria"]?.jsonString ?? ""
                )
            }
            let survey = try await store.resolveRequirementSurvey(
                ownerUserID: ownerUserID,
                projectID: projectID,
                surveyID: try requiredString(arguments, "survey_id"),
                resolverAgentID: creatorAgentID,
                resolution: .init(
                    summary: try requiredString(arguments, "summary"),
                    solutionMarkdown: try requiredString(arguments, "solution_markdown"),
                    executionSteps: steps,
                    risksAndOpenQuestions: arguments["risks_and_open_questions"]?.jsonString ?? "",
                    relatedMaterials: arguments["related_materials"]?.jsonString ?? ""
                ),
                nowUnixMs: now()
            )
            return detail(survey)

        default:
            throw ToolError.invalid("当前任务没有这个需求调研工具：\(name)")
        }
    }

    private func summary(_ survey: LocalAgentRequirementSurvey) -> NativeJSONValue {
        .object([
            "survey_id": .string(survey.id),
            "title": .string(survey.draft.title),
            "purpose": .string(survey.draft.purpose),
            "status": .string(survey.status.rawValue),
            "has_submission": .bool(survey.submission != nil),
            "has_resolution": .bool(survey.resolution != nil),
            "created_at_unix_ms": .number(Double(survey.createdAtUnixMs)),
            "submitted_at_unix_ms": numberOrNull(survey.submittedAtUnixMs),
            "resolved_at_unix_ms": numberOrNull(survey.resolvedAtUnixMs),
        ])
    }

    private func resource(
        _ value: LocalAgentProgressiveSkillCatalog.Resource
    ) -> NativeJSONValue {
        .object([
            "relative_path": .string(value.relativePath),
            "kind": .string(value.kind),
            "size_bytes": .number(Double(value.sizeBytes)),
            "sha256": .string(value.sha256),
        ])
    }

    private func detail(_ survey: LocalAgentRequirementSurvey) -> NativeJSONValue {
        let answers = Dictionary(uniqueKeysWithValues: (survey.submission?.answers ?? []).map {
            ($0.questionID, $0.selectedOptionIDs)
        })
        return .object([
            "survey_id": .string(survey.id),
            "project_bound": .bool(true),
            "title": .string(survey.draft.title),
            "purpose": .string(survey.draft.purpose),
            "status": .string(survey.status.rawValue),
            "questions": .array(survey.draft.questions.map { question in
                let selected = answers[question.id] ?? []
                return .object([
                    "key": .string(question.id),
                    "prompt": .string(question.prompt),
                    "kind": .string(question.kind.rawValue),
                    "required": .bool(question.isRequired),
                    "options": .array(question.options.map { option in
                        .object([
                            "key": .string(option.id),
                            "label": .string(option.label),
                            "selected": .bool(selected.contains(option.id)),
                        ])
                    }),
                    "selected_option_keys": .array(selected.map(NativeJSONValue.string)),
                ])
            }),
            "notes": survey.submission.map { .string($0.notes) } ?? .null,
            "resolution": survey.resolution.map(resolution) ?? .null,
            "created_at_unix_ms": .number(Double(survey.createdAtUnixMs)),
            "submitted_at_unix_ms": numberOrNull(survey.submittedAtUnixMs),
            "resolved_at_unix_ms": numberOrNull(survey.resolvedAtUnixMs),
        ])
    }

    private func resolution(_ value: LocalAgentRequirementSurveyResolution) -> NativeJSONValue {
        .object([
            "summary": .string(value.summary),
            "solution_markdown": .string(value.solutionMarkdown),
            "execution_steps": .array(value.executionSteps.map { step in
                .object([
                    "key": .string(step.id), "title": .string(step.title),
                    "detail": .string(step.detail), "owner": .string(step.owner),
                    "deliverable": .string(step.deliverable),
                    "acceptance_criteria": .string(step.acceptanceCriteria),
                ])
            }),
            "risks_and_open_questions": .string(value.risksAndOpenQuestions),
            "related_materials": .string(value.relatedMaterials),
        ])
    }

    private func task(_ todo: LocalAgentTodo) -> NativeJSONValue {
        .object([
            "task_id": .string(todo.id),
            "title": .string(todo.title),
            "objective": .string(todo.executionContract.objective),
            "scope": .string(todo.executionContract.scope),
            "status": .string(todo.status.rawValue),
            "assignee_agent_id": .string(todo.agentID),
            "priority": .number(Double(todo.priority)),
            "blocked_reason": .string(todo.blockedReason),
            "result": .string(todo.result),
            "builtin_capabilities": .array(
                todo.executionPlan.builtinCapabilities.map { .string($0.rawValue) }
            ),
            "plugins": .array(todo.executionPlan.plugins.map { .string($0.displayName) }),
            "updated_at_unix_ms": .number(Double(todo.updatedAtUnixMs)),
        ])
    }

    private static func definition(
        name: String,
        description: String,
        properties: [String: NativeJSONValue] = [:],
        required: [String] = []
    ) -> NativeJSONValue {
        .object([
            "name": .string(name),
            "description": .string(description),
            "inputSchema": .object([
                "type": .string("object"),
                "properties": .object(properties),
                "required": .array(required.map(NativeJSONValue.string)),
                "additionalProperties": .bool(false),
            ]),
        ])
    }

    private static func stringSchema(maximum: Int) -> NativeJSONValue {
        .object([
            "type": .string("string"), "minLength": .number(1),
            "maxLength": .number(Double(maximum)),
        ])
    }

    private static func optionalStringSchema(maximum: Int) -> NativeJSONValue {
        .object([
            "type": .string("string"), "maxLength": .number(Double(maximum)),
        ])
    }

    private func requiredString(
        _ arguments: [String: NativeJSONValue],
        _ key: String
    ) throws -> String {
        guard let value = arguments[key]?.jsonString,
              !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw ToolError.invalid("\(key) 不能为空")
        }
        return value
    }

    private func requiredArray(
        _ arguments: [String: NativeJSONValue],
        _ key: String
    ) throws -> [NativeJSONValue] {
        guard let value = arguments[key]?.jsonArray else {
            throw ToolError.invalid("\(key) 必须是数组")
        }
        return value
    }

    private func numberOrNull(_ value: Int64?) -> NativeJSONValue {
        value.map { .number(Double($0)) } ?? .null
    }

    private enum ToolError: LocalizedError {
        case invalid(String)
        case notFound

        var errorDescription: String? {
            switch self {
            case let .invalid(message): message
            case .notFound: "当前项目中没有这张需求调研单，请重新调用 requirement_survey_list。"
            }
        }
    }
}
