namespace ChatOS.Core.Domain;

public enum AgentTodoStatus
{
    Pending,
    Ready,
    InProgress,
    Blocked,
    Completed,
    Cancelled,
}

public enum AgentTodoPriority
{
    Low,
    Normal,
    High,
    Urgent,
}

public sealed record AgentTodoDraft(
    string TeamRoomId,
    string AgentId,
    string Title,
    string Detail = "",
    AgentTodoPriority Priority = AgentTodoPriority.Normal,
    IReadOnlyList<string>? DependencyIds = null,
    string? SourceMessageId = null)
{
    public IReadOnlyList<string> Dependencies => DependencyIds ?? [];

    public void Validate()
    {
        AgentTeamValidation.Identifier(TeamRoomId, nameof(TeamRoomId));
        AgentTeamValidation.Identifier(AgentId, nameof(AgentId));
        AgentTeamValidation.Text(Title, nameof(Title), 500);
        AgentTeamValidation.OptionalText(Detail, nameof(Detail), 16_000);
        AgentTeamValidation.Identifiers(Dependencies, nameof(DependencyIds), 100);
        if (SourceMessageId is not null)
        {
            AgentTeamValidation.Identifier(SourceMessageId, nameof(SourceMessageId));
        }
    }
}

public sealed record AgentTodo(
    string Id,
    string OwnerUserId,
    AgentTodoDraft Draft,
    AgentTodoStatus Status,
    string Result,
    int SortOrder,
    long Revision,
    long CreatedAtUnixMs,
    long UpdatedAtUnixMs)
{
    public bool IsTerminal => Status is AgentTodoStatus.Completed or AgentTodoStatus.Cancelled;

    public void Validate()
    {
        AgentTeamValidation.Identifier(Id, nameof(Id));
        AgentTeamValidation.Identifier(OwnerUserId, nameof(OwnerUserId));
        Draft.Validate();
        AgentTeamValidation.OptionalText(Result, nameof(Result), 16_000);
        if (SortOrder < 0 || Revision <= 0)
        {
            throw AgentTeamValidation.Invalid("todo ordering");
        }

        AgentTeamValidation.Timestamps(CreatedAtUnixMs, UpdatedAtUnixMs);
    }
}

public enum AgentTodoProgressKind
{
    Started,
    Update,
    Blocked,
    Completed,
    Failed,
}

public sealed record AgentTeamAssetUpdateSuggestion(
    AgentTeamAssetCategory Category,
    string Title,
    string Markdown,
    string Rationale)
{
    public void Validate()
    {
        AgentTeamValidation.Text(Title, nameof(Title), 240);
        AgentTeamValidation.Text(Markdown, nameof(Markdown), 128_000);
        AgentTeamValidation.Text(Rationale, nameof(Rationale), 4_000);
    }
}

public sealed record AgentTodoProgress(
    string Id,
    string OwnerUserId,
    string AgentId,
    string TodoId,
    long Sequence,
    AgentTodoProgressKind Kind,
    string Stage,
    string Detail,
    long CreatedAtUnixMs,
    IReadOnlyList<AgentTeamAssetUpdateSuggestion>? AssetUpdateSuggestions = null)
{
    public IReadOnlyList<AgentTeamAssetUpdateSuggestion> Suggestions => AssetUpdateSuggestions ?? [];
}

public enum AgentTeamAssetCategory
{
    Overview,
    CurrentProgress,
    TechStack,
    Architecture,
    Conventions,
    Requirement,
    Plan,
    Decision,
    Research,
    Deliverable,
    Note,
    Reference,
}

public enum AgentTeamAssetStatus
{
    Active,
    Archived,
}

public sealed record AgentTeamAsset(
    string Id,
    string OwnerUserId,
    string TeamRoomId,
    AgentTeamAssetCategory Category,
    string Title,
    string Markdown,
    AgentTeamAssetStatus Status,
    int Revision,
    string? CreatedByAgentId,
    string? UpdatedByAgentId,
    long CreatedAtUnixMs,
    long UpdatedAtUnixMs)
{
    public void Validate()
    {
        AgentTeamValidation.Identifier(Id, nameof(Id));
        AgentTeamValidation.Identifier(OwnerUserId, nameof(OwnerUserId));
        AgentTeamValidation.Identifier(TeamRoomId, nameof(TeamRoomId));
        AgentTeamValidation.Text(Title, nameof(Title), 240);
        AgentTeamValidation.OptionalText(Markdown, nameof(Markdown), 256_000);
        if (Revision <= 0)
        {
            throw AgentTeamValidation.Invalid(nameof(Revision));
        }

        AgentTeamValidation.Timestamps(CreatedAtUnixMs, UpdatedAtUnixMs);
    }
}

public sealed record AgentTeamAssetRevision(
    string AssetId,
    int Revision,
    string Title,
    string Markdown,
    AgentTeamAssetStatus Status,
    string? EditorAgentId,
    long CreatedAtUnixMs);

public enum AgentDeliveryTrigger
{
    Mention,
    DefaultAgent,
    AgentMention,
    Heartbeat,
    Todo,
    TodoStatus,
    RequirementSurvey,
}

public enum AgentDeliveryStatus
{
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

public sealed record AgentDelivery(
    string Id,
    string OwnerUserId,
    string RoomId,
    string MessageId,
    string RootMessageId,
    string TargetAgentId,
    AgentDeliveryTrigger Trigger,
    AgentDeliveryStatus Status,
    int Attempt,
    int HopCount,
    string DeduplicationKey,
    string? ResponseMessageId,
    string? LastError,
    long? ClaimedAtUnixMs,
    long? CompletedAtUnixMs,
    long CreatedAtUnixMs);

public enum AgentRunStatus
{
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

public sealed record AgentRunSummary(
    string Id,
    string OwnerUserId,
    string DeliveryId,
    string AgentId,
    string RoomId,
    AgentRunStatus Status,
    int ModelCalls,
    string? LastError,
    long CreatedAtUnixMs,
    long UpdatedAtUnixMs);

public sealed record AgentTeamSnapshot(
    AgentRoom Room,
    IReadOnlyList<AgentRoomMember> Members,
    IReadOnlyList<AgentProfile> Profiles,
    IReadOnlyList<AgentMessage> Messages,
    IReadOnlyList<AgentTodo> Todos,
    IReadOnlyList<AgentTeamAsset> Assets,
    IReadOnlyList<AgentRequirementSurvey> RequirementSurveys,
    IReadOnlyList<AgentRunSummary> Runs);
