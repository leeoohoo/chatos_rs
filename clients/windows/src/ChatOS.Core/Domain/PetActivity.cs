namespace ChatOS.Core.Domain;

public enum PetActivitySource
{
    LocalAgentToolApproval,
    AskUserPrompt,
    Chat,
    TaskRunner,
}

public enum PetActivityKind
{
    Working,
    WaitingForApproval,
    WaitingForUser,
    NeedsReview,
    Succeeded,
    Failed,
    Cancelled,
}

public enum PetActivityDisposition
{
    Ignored,
    Handled,
}

public enum PetAnimationState
{
    Idle,
    Running,
    Review,
    Waiting,
    Succeeded,
    Failed,
}

public sealed record PetActivityRoute(
    string? ProjectId = null,
    string? ConversationId = null,
    string? TurnId = null,
    string? MessageId = null,
    string? PromptId = null,
    string? TaskId = null,
    string? RunId = null,
    string? InvocationId = null);

public sealed record PetActivity
{
    public PetActivity(
        string id,
        PetActivitySource source,
        PetActivityKind kind,
        string title,
        string? detail = null,
        PetActivityRoute? route = null,
        string? activityVersion = null,
        DateTimeOffset? updatedAt = null,
        DateTimeOffset? expiresAt = null)
    {
        Id = id;
        Source = source;
        Kind = kind;
        Title = title;
        Detail = detail;
        Route = route ?? new PetActivityRoute();
        ActivityVersion = activityVersion;
        UpdatedAt = updatedAt ?? DateTimeOffset.UtcNow;
        ExpiresAt = expiresAt;
    }

    public string Id { get; init; }

    public PetActivitySource Source { get; init; }

    public PetActivityKind Kind { get; init; }

    public string Title { get; init; }

    public string? Detail { get; init; }

    public PetActivityRoute Route { get; init; }

    public string? ActivityVersion { get; init; }

    public DateTimeOffset UpdatedAt { get; init; }

    public DateTimeOffset? ExpiresAt { get; init; }

    public bool RequiresAttention => Kind is
        PetActivityKind.WaitingForApproval or
        PetActivityKind.WaitingForUser or
        PetActivityKind.NeedsReview or
        PetActivityKind.Failed;

    public int PresentationPriority => Kind switch
    {
        PetActivityKind.WaitingForApproval or PetActivityKind.WaitingForUser => 500,
        PetActivityKind.Failed or PetActivityKind.NeedsReview => 400,
        PetActivityKind.Succeeded => 300,
        PetActivityKind.Working => 100,
        PetActivityKind.Cancelled => 50,
        _ => 0,
    };

    public PetAnimationState AnimationState => Kind switch
    {
        PetActivityKind.Working => PetAnimationState.Running,
        PetActivityKind.NeedsReview => PetAnimationState.Review,
        PetActivityKind.WaitingForApproval or PetActivityKind.WaitingForUser => PetAnimationState.Waiting,
        PetActivityKind.Succeeded => PetAnimationState.Succeeded,
        PetActivityKind.Failed => PetAnimationState.Failed,
        _ => PetAnimationState.Idle,
    };

    public string StableIdentity
    {
        get
        {
            var version = ActivityVersion ?? Route.RunId ?? Route.TurnId ?? "1";
            return $"{Source}|{Id}|{version}";
        }
    }
}

public sealed record PetPresentation(
    PetAnimationState AnimationState,
    PetActivity? PrimaryActivity,
    int ActiveWorkCount,
    int AttentionCount)
{
    public static PetPresentation Idle { get; } = new(
        PetAnimationState.Idle,
        null,
        0,
        0);
}
