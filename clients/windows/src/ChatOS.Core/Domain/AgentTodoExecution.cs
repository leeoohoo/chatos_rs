using System.Text.Json.Serialization;

namespace ChatOS.Core.Domain;

public enum AgentTodoBuiltinCapability
{
    ProjectRead,
    ProjectWrite,
    Terminal,
    RequirementSurveyRead,
    RequirementSurveyWrite,
}

public sealed record AgentTodoPluginSelection(
    string PluginId,
    string DisplayName,
    string Reason = "")
{
    public void Validate()
    {
        AgentTeamValidation.Identifier(PluginId, nameof(PluginId));
        AgentTeamValidation.Text(DisplayName, nameof(DisplayName), 240);
        AgentTeamValidation.OptionalText(Reason, nameof(Reason), 1_000);
    }
}

public sealed record AgentTodoExecutionPlan(
    bool RequiresExecution = true,
    IReadOnlyList<AgentTodoBuiltinCapability>? BuiltinCapabilities = null,
    IReadOnlyList<AgentTodoPluginSelection>? Plugins = null,
    string SelectionRevision = "windows-capability-catalog-v1",
    long SelectedAtUnixMs = 0)
{
    [JsonIgnore]
    public IReadOnlyList<AgentTodoBuiltinCapability> Capabilities =>
        CompleteDependencies(BuiltinCapabilities ?? [AgentTodoBuiltinCapability.ProjectRead]);

    [JsonIgnore]
    public IReadOnlyList<AgentTodoPluginSelection> SelectedPlugins => Plugins ?? [];

    public AgentTodoExecutionPlan Normalized(long selectedAtUnixMs) => this with
    {
        BuiltinCapabilities = Capabilities,
        Plugins = SelectedPlugins,
        SelectedAtUnixMs = SelectedAtUnixMs == 0 ? selectedAtUnixMs : SelectedAtUnixMs,
    };

    public void Validate()
    {
        var requestedCapabilities = BuiltinCapabilities ?? [AgentTodoBuiltinCapability.ProjectRead];
        if (requestedCapabilities.Count > Enum.GetValues<AgentTodoBuiltinCapability>().Length ||
            requestedCapabilities.Distinct().Count() != requestedCapabilities.Count ||
            SelectedPlugins.Count > 32 ||
            SelectedPlugins.Select(value => value.PluginId).Distinct(StringComparer.Ordinal).Count() !=
            SelectedPlugins.Count || SelectedAtUnixMs < 0)
        {
            throw AgentTeamValidation.Invalid(nameof(AgentTodoExecutionPlan));
        }

        if (Capabilities.Contains(AgentTodoBuiltinCapability.RequirementSurveyWrite) &&
            !Capabilities.Contains(AgentTodoBuiltinCapability.RequirementSurveyRead))
        {
            throw AgentTeamValidation.Invalid(nameof(BuiltinCapabilities));
        }

        if (!RequiresExecution && Capabilities.Any(value => value is not
            (AgentTodoBuiltinCapability.ProjectRead or
             AgentTodoBuiltinCapability.RequirementSurveyRead)))
        {
            throw AgentTeamValidation.Invalid(nameof(RequiresExecution));
        }

        AgentTeamValidation.Identifier(SelectionRevision, nameof(SelectionRevision));
        foreach (var plugin in SelectedPlugins) plugin.Validate();
    }

    private static IReadOnlyList<AgentTodoBuiltinCapability> CompleteDependencies(
        IReadOnlyList<AgentTodoBuiltinCapability> capabilities)
    {
        var result = new List<AgentTodoBuiltinCapability>();
        foreach (var capability in capabilities)
        {
            if (capability == AgentTodoBuiltinCapability.RequirementSurveyWrite &&
                !result.Contains(AgentTodoBuiltinCapability.RequirementSurveyRead))
            {
                result.Add(AgentTodoBuiltinCapability.RequirementSurveyRead);
            }

            if (!result.Contains(capability)) result.Add(capability);
        }

        return result;
    }
}

public sealed record AgentTodoExecutionContract(
    string Objective = "",
    string Scope = "",
    IReadOnlyList<string>? ExpectedOutputs = null,
    IReadOnlyList<string>? AcceptanceCriteria = null,
    IReadOnlyList<string>? Constraints = null)
{
    [JsonIgnore]
    public IReadOnlyList<string> Outputs => ExpectedOutputs ?? [];
    [JsonIgnore]
    public IReadOnlyList<string> Criteria => AcceptanceCriteria ?? [];
    [JsonIgnore]
    public IReadOnlyList<string> Limits => Constraints ?? [];

    public AgentTodoExecutionContract Normalized(string title, string detail) => this with
    {
        Objective = string.IsNullOrWhiteSpace(Objective) ? title : Objective,
        Scope = string.IsNullOrWhiteSpace(Scope) ? detail : Scope,
        ExpectedOutputs = Outputs.Count == 0 ? [title] : Outputs,
        AcceptanceCriteria = Criteria.Count == 0
            ? ["完成任务目标并输出可核验的结果总结。"]
            : Criteria,
        Constraints = Limits,
    };

    public void Validate()
    {
        AgentTeamValidation.Text(Objective, nameof(Objective), 8_000);
        AgentTeamValidation.OptionalText(Scope, nameof(Scope), 16_000);
        ValidateList(Outputs, nameof(ExpectedOutputs), allowEmpty: false);
        ValidateList(Criteria, nameof(AcceptanceCriteria), allowEmpty: false);
        ValidateList(Limits, nameof(Constraints), allowEmpty: true);
    }

    private static void ValidateList(IReadOnlyList<string> values, string field, bool allowEmpty)
    {
        if (values.Count > 64 || !allowEmpty && values.Count == 0)
            throw AgentTeamValidation.Invalid(field);
        foreach (var value in values) AgentTeamValidation.Text(value, field, 4_000);
    }
}

public enum AgentTodoSourceRelation
{
    Created,
    Updated,
    Reprioritized,
    BlockedContext,
}

public sealed record AgentTodoSourceDraft(
    string ConversationId,
    string MessageId,
    AgentTodoSourceRelation Relation = AgentTodoSourceRelation.Created)
{
    public void Validate()
    {
        AgentTeamValidation.Identifier(ConversationId, nameof(ConversationId));
        AgentTeamValidation.Identifier(MessageId, nameof(MessageId));
    }
}

public sealed record AgentTodoSourceLink(
    string TodoId,
    string ConversationId,
    string MessageId,
    AgentTodoSourceRelation Relation,
    long CreatedAtUnixMs);
