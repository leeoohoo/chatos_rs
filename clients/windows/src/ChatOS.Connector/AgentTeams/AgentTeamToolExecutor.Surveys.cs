using System.Text.Json;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.AgentTeams;

internal sealed partial class AgentTeamToolExecutor
{
    public static IReadOnlyList<AgentToolDefinition> SurveyDefinitions { get; } =
    [
        Tool("skill_activate",
            "激活需求调研 Skill Catalog 中的一个不可变 Skill；先激活 Router，再按其路由激活一个专业 Skill。",
            new
            {
                type = "object",
                properties = new
                {
                    skill_ref = new { type = "string", maxLength = 80 },
                },
                required = new[] { "skill_ref" },
                additionalProperties = false,
            }),
        Tool("skill_list_resources", "列出一个需求调研 Skill 声明的按需资源。", new
        {
            type = "object",
            properties = new { skill_ref = new { type = "string", maxLength = 80 } },
            required = new[] { "skill_ref" },
            additionalProperties = false,
        }),
        Tool("skill_read_resource", "按需分段读取需求调研 Skill 的文本资源。", new
        {
            type = "object",
            properties = new
            {
                skill_ref = new { type = "string", maxLength = 80 },
                relative_path = new { type = "string", maxLength = 1_000 },
                offset = new { type = "integer", minimum = 0 },
                max_chars = new { type = "integer", minimum = 1, maximum = 64_000 },
            },
            required = new[] { "skill_ref", "relative_path" },
            additionalProperties = false,
        }),
        Tool("requirement_survey_create",
            "项目经理创建 1–12 题的 Human 单选/多选需求调研。创建后本轮结束并等待 Human。",
            new
            {
                type = "object",
                properties = new
                {
                    request_key = new { type = "string", maxLength = 512 },
                    title = new { type = "string", maxLength = 240 },
                    purpose = new { type = "string", maxLength = 4_000 },
                    questions = new
                    {
                        type = "array", minItems = 1, maxItems = 12,
                        items = new
                        {
                            type = "object",
                            properties = new
                            {
                                key = new { type = "string" },
                                prompt = new { type = "string", maxLength = 1_000 },
                                kind = new { type = "string", @enum = new[] { "single_choice", "multiple_choice" } },
                                required = new { type = "boolean" },
                                options = new
                                {
                                    type = "array", minItems = 2, maxItems = 12,
                                    items = new
                                    {
                                        type = "object",
                                        properties = new
                                        {
                                            key = new { type = "string" },
                                            label = new { type = "string", maxLength = 500 },
                                        },
                                        required = new[] { "key", "label" },
                                        additionalProperties = false,
                                    },
                                },
                            },
                            required = new[] { "key", "prompt", "kind", "options" },
                            additionalProperties = false,
                        },
                    },
                },
                required = new[] { "request_key", "title", "purpose", "questions" },
                additionalProperties = false,
            }),
        Tool("requirement_survey_list", "列出当前团队需求调研；Human 提交后先调用它取得真实状态。", new
        {
            type = "object",
            properties = new
            {
                status = new { type = "string", @enum = Enum.GetNames<AgentRequirementSurveyStatus>() },
            },
            additionalProperties = false,
        }),
        Tool("requirement_survey_get", "读取一张需求调研的题目、Human 真实答案、备注与解决方案。", new
        {
            type = "object",
            properties = new { survey_ref = new { type = "string" } },
            required = new[] { "survey_ref" },
            additionalProperties = false,
        }),
        Tool("requirement_survey_project_tasks",
            "读取当前项目全部团队的 Todo 事实，用于核对调研执行计划。", ObjectSchema()),
        Tool("requirement_survey_resolve", "Human 提交后由项目经理形成解决方案、执行步骤、风险和资料。", new
        {
            type = "object",
            properties = new
            {
                survey_ref = new { type = "string" },
                summary = new { type = "string", maxLength = 4_000 },
                solution_markdown = new { type = "string", maxLength = 128_000 },
                execution_steps = new
                {
                    type = "array", minItems = 1, maxItems = 50,
                    items = new
                    {
                        type = "object",
                        properties = new
                        {
                            key = new { type = "string" },
                            title = new { type = "string", maxLength = 500 },
                            detail = new { type = "string", maxLength = 8_000 },
                            owner = new { type = "string", maxLength = 500 },
                            deliverable = new { type = "string", maxLength = 4_000 },
                            acceptance_criteria = new { type = "string", maxLength = 4_000 },
                        },
                        required = new[] { "key", "title", "detail" },
                        additionalProperties = false,
                    },
                },
                risks_and_open_questions = new { type = "string", maxLength = 32_000 },
                related_materials = new { type = "string", maxLength = 32_000 },
            },
            required = new[] { "survey_ref", "summary", "solution_markdown", "execution_steps" },
            additionalProperties = false,
        }),
    ];

    private static AgentToolExecutionResult ActivateSkill(JsonElement arguments)
    {
        try
        {
            return new AgentToolExecutionResult(Json(
                AgentRequirementSurveySkillCatalog.Activate(
                    RequiredString(arguments, "skill_ref"))));
        }
        catch (KeyNotFoundException)
        {
            throw AgentTeamValidation.Invalid("skill_ref");
        }
    }

    private static AgentToolExecutionResult ListSkillResources(JsonElement arguments)
    {
        try
        {
            return new AgentToolExecutionResult(Json(
                AgentRequirementSurveySkillCatalog.ListResources(
                    RequiredString(arguments, "skill_ref"))));
        }
        catch (KeyNotFoundException)
        {
            throw AgentTeamValidation.Invalid("skill_ref");
        }
    }

    private static AgentToolExecutionResult ReadSkillResource(JsonElement arguments)
    {
        try
        {
            return new AgentToolExecutionResult(Json(
                AgentRequirementSurveySkillCatalog.ReadResource(
                    RequiredString(arguments, "skill_ref"),
                    RequiredString(arguments, "relative_path"),
                    OptionalInt(arguments, "offset") ?? 0,
                    OptionalInt(arguments, "max_chars") ?? 32_000)));
        }
        catch (Exception exception) when (exception is KeyNotFoundException or ArgumentException)
        {
            throw new AgentTeamException(AgentTeamError.InvalidField,
                "Skill reference, resource path, or range is invalid.", exception);
        }
    }

    private async Task<AgentToolExecutionResult> CreateRequirementSurveyAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentDelivery delivery,
        AgentRunReferenceVault references,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        RequireSurveyCapability(profile, room);
        var questions = ObjectArray(arguments, "questions", 12).Select((question, index) =>
        {
            var kind = RequiredString(question, "kind") switch
            {
                "single_choice" => AgentRequirementQuestionKind.SingleChoice,
                "multiple_choice" => AgentRequirementQuestionKind.MultipleChoice,
                _ => throw AgentTeamValidation.Invalid($"questions[{index}].kind"),
            };
            var options = ObjectArray(question, "options", 12)
                .Select(option => new AgentRequirementOption(
                    RequiredString(option, "key"), RequiredString(option, "label"))).ToArray();
            return new AgentRequirementQuestion(RequiredString(question, "key"),
                RequiredString(question, "prompt"), kind, options,
                OptionalBool(question, "required") ?? true);
        }).ToArray();
        var survey = await store.CreateRequirementSurveyAsync(profile.OwnerUserId, room.Id,
            profile.Id, delivery.Id, RequiredString(arguments, "request_key"),
            new AgentRequirementSurveyDraft(RequiredString(arguments, "title"),
                RequiredString(arguments, "purpose"), questions), cancellationToken)
            .ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(new
        {
            project_bound = true,
            survey_ref = references.SurveyReference(room.ProjectId, survey.Id),
            survey.Status,
        }),
            EndsCycle: true);
    }

    private async Task<AgentToolExecutionResult> ListRequirementSurveysAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentRunReferenceVault references,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        RequireSurveyCapability(profile, room);
        var statusText = OptionalString(arguments, "status");
        AgentRequirementSurveyStatus? status = statusText is null
            ? null
            : Enum.TryParse<AgentRequirementSurveyStatus>(statusText, true, out var parsed)
                ? parsed
                : throw AgentTeamValidation.Invalid("status");
        var surveys = await store.ListRequirementSurveysAsync(profile.OwnerUserId, room.ProjectId,
            status, cancellationToken: cancellationToken).ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(new
        {
            project_bound = true,
            surveys = surveys.Select(value => new
            {
                survey_ref = references.SurveyReference(room.ProjectId, value.Id),
                value.Draft.Title,
                value.Draft.Purpose,
                value.Status,
                value.CreatedAtUnixMs,
                value.SubmittedAtUnixMs,
                resolved = value.Resolution is not null,
            }),
        }));
    }

    private async Task<AgentToolExecutionResult> GetRequirementSurveyAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentRunReferenceVault references,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        RequireSurveyCapability(profile, room);
        var surveyReference = RequiredString(arguments, "survey_ref");
        var authority = references.Survey(surveyReference);
        var surveyId = authority?.SurveyId ?? (references.AllowsLegacyIds
            ? surveyReference : throw AgentTeamValidation.Invalid("survey_ref"));
        if (authority is not null && authority.ProjectId != room.ProjectId)
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Survey reference does not belong to the current project.");
        var survey = await store.GetRequirementSurveyAsync(profile.OwnerUserId, room.ProjectId,
            surveyId, cancellationToken).ConfigureAwait(false)
            ?? throw new AgentTeamException(AgentTeamError.NotFound,
                "Requirement survey was not found.");
        return new AgentToolExecutionResult(Json(new
        {
            project_bound = true,
            survey_ref = references.SurveyReference(room.ProjectId, survey.Id),
            survey.Draft,
            survey.Status,
            survey.Submission,
            survey.Resolution,
            survey.CreatedAtUnixMs,
            survey.SubmittedAtUnixMs,
            survey.ResolvedAtUnixMs,
        }));
    }

    private async Task<AgentToolExecutionResult> ListRequirementSurveyProjectTasksAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentRunReferenceVault references,
        CancellationToken cancellationToken)
    {
        RequireSurveyCapability(profile, room);
        var todos = await store.ListProjectTodosAsync(profile.OwnerUserId, room.ProjectId,
            includeTerminal: true, limit: 200, cancellationToken).ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(new
        {
            project_bound = true,
            teams = todos.GroupBy(value => value.Draft.TeamRoomId).Select(group => new
            {
                team_ref = references.ConversationReference(group.Key),
                todos = group.Select(value => new
                {
                    todo_ref = references.TodoReference(group.Key, value.Id,
                        value.Draft.AgentId),
                    value.Draft.Title,
                    value.Draft.Detail,
                    assignee_ref = references.AgentReference(value.Draft.AgentId),
                    value.Status,
                    value.Result,
                    value.Revision,
                }),
            }),
        }));
    }

    private async Task<AgentToolExecutionResult> ResolveRequirementSurveyAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentRunReferenceVault references,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        RequireSurveyCapability(profile, room);
        var steps = ObjectArray(arguments, "execution_steps", 50).Select(step =>
            new AgentRequirementExecutionStep(RequiredString(step, "key"),
                RequiredString(step, "title"), RequiredString(step, "detail"),
                OptionalString(step, "owner") ?? string.Empty,
                OptionalString(step, "deliverable") ?? string.Empty,
                OptionalString(step, "acceptance_criteria") ?? string.Empty)).ToArray();
        var resolution = new AgentRequirementResolution(
            RequiredString(arguments, "summary"),
            RequiredString(arguments, "solution_markdown"), steps,
            OptionalString(arguments, "risks_and_open_questions") ?? string.Empty,
            OptionalString(arguments, "related_materials") ?? string.Empty);
        var surveyReference = RequiredString(arguments, "survey_ref");
        var authority = references.Survey(surveyReference);
        var surveyId = authority?.SurveyId ?? (references.AllowsLegacyIds
            ? surveyReference : throw AgentTeamValidation.Invalid("survey_ref"));
        if (authority is not null && authority.ProjectId != room.ProjectId)
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "Survey reference does not belong to the current project.");
        var survey = await store.ResolveRequirementSurveyAsync(profile.OwnerUserId, room.ProjectId,
            surveyId, profile.Id, resolution, cancellationToken)
            .ConfigureAwait(false);
        return new AgentToolExecutionResult(Json(new
        {
            project_bound = true,
            survey_ref = references.SurveyReference(room.ProjectId, survey.Id),
            resolved = true,
        }),
            EndsCycle: true);
    }

    private static IReadOnlyList<JsonElement> ObjectArray(
        JsonElement value,
        string name,
        int maximumCount)
    {
        if (!value.TryGetProperty(name, out var property) || property.ValueKind != JsonValueKind.Array)
            throw AgentTeamValidation.Invalid(name);
        var values = property.EnumerateArray().ToArray();
        if (values.Length == 0 || values.Length > maximumCount ||
            values.Any(item => item.ValueKind != JsonValueKind.Object))
            throw AgentTeamValidation.Invalid(name);
        return values;
    }

    private static IReadOnlyList<JsonElement> OptionalObjectArray(
        JsonElement value,
        string name,
        int maximumCount)
    {
        if (!value.TryGetProperty(name, out var property) || property.ValueKind == JsonValueKind.Null)
            return [];
        if (property.ValueKind != JsonValueKind.Array) throw AgentTeamValidation.Invalid(name);
        var values = property.EnumerateArray().ToArray();
        if (values.Length > maximumCount || values.Any(item => item.ValueKind != JsonValueKind.Object))
            throw AgentTeamValidation.Invalid(name);
        return values;
    }

    private static bool? OptionalBool(JsonElement value, string name) =>
        value.TryGetProperty(name, out var property) &&
        property.ValueKind is JsonValueKind.True or JsonValueKind.False
            ? property.GetBoolean()
            : null;

    private static void RequireSurveyCapability(AgentProfile profile, AgentRoom room)
    {
        if (!CanManageSurveys(profile, room))
        {
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "The Agent is not allowed to manage requirement surveys for this team.");
        }
    }

    private static bool CanManageSurveys(AgentProfile profile, AgentRoom room) =>
        room.Kind == AgentConversationKind.ProjectTeam &&
            (string.Equals(room.ProjectManagerAgentId, profile.Id, StringComparison.Ordinal) ||
             string.Equals(profile.Draft.ProfessionKey, "project_manager",
                 StringComparison.Ordinal) ||
             profile.Draft.Skills.Contains("requirement.survey.manage",
                 StringComparer.Ordinal));
}
