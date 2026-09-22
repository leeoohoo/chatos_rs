namespace ChatOS.Core.Domain;

public enum AgentProfileStatus
{
    Active,
    Archived,
}

public sealed record AgentProfileDraft(
    string Name,
    string Description,
    string RolePrompt,
    string ModelConfigId,
    string? ThinkingLevel = null,
    string ProfessionKey = "general",
    IReadOnlyList<string>? DefaultPluginIds = null,
    IReadOnlyList<string>? DefaultSkillIds = null,
    bool HeartbeatEnabled = false,
    int HeartbeatIntervalSeconds = 900,
    string HeartbeatPrompt = "")
{
    public IReadOnlyList<string> Plugins => DefaultPluginIds ?? [];

    public IReadOnlyList<string> Skills => DefaultSkillIds ?? [];

    public void Validate()
    {
        AgentTeamValidation.Text(Name, nameof(Name), 120);
        AgentTeamValidation.OptionalText(Description, nameof(Description), 2_000);
        AgentTeamValidation.Text(RolePrompt, nameof(RolePrompt), 32_000);
        AgentTeamValidation.Identifier(ModelConfigId, nameof(ModelConfigId));
        AgentTeamValidation.OptionalText(ThinkingLevel, nameof(ThinkingLevel), 32);
        AgentTeamValidation.Identifier(ProfessionKey, nameof(ProfessionKey));
        AgentTeamValidation.Identifiers(Plugins, nameof(DefaultPluginIds), 100);
        AgentTeamValidation.Identifiers(Skills, nameof(DefaultSkillIds), 100);
        if (HeartbeatIntervalSeconds is < 60 or > 86_400)
        {
            throw AgentTeamValidation.Invalid(nameof(HeartbeatIntervalSeconds));
        }

        AgentTeamValidation.OptionalText(HeartbeatPrompt, nameof(HeartbeatPrompt), 8_000);
    }
}

public sealed record AgentProfile(
    string Id,
    string OwnerUserId,
    AgentProfileDraft Draft,
    AgentProfileStatus Status,
    long CreatedAtUnixMs,
    long UpdatedAtUnixMs,
    long? LastHeartbeatAtUnixMs = null,
    long? NextHeartbeatAtUnixMs = null)
{
    public void Validate()
    {
        AgentTeamValidation.Identifier(Id, nameof(Id));
        AgentTeamValidation.Identifier(OwnerUserId, nameof(OwnerUserId));
        Draft.Validate();
        AgentTeamValidation.Timestamps(CreatedAtUnixMs, UpdatedAtUnixMs);
        if (LastHeartbeatAtUnixMs is < 0 || NextHeartbeatAtUnixMs is < 0)
        {
            throw AgentTeamValidation.Invalid("heartbeat timestamps");
        }
    }
}

public enum AgentTeamError
{
    InvalidField,
    NotFound,
    Conflict,
    NotMember,
    PermissionDenied,
    Storage,
    ModelUnavailable,
}

public sealed class AgentTeamException(
    AgentTeamError code,
    string message,
    Exception? innerException = null,
    bool isTransient = false) : Exception(message, innerException)
{
    public AgentTeamError Code { get; } = code;
    public bool IsTransient { get; } = isTransient;
}

public static class AgentTeamValidation
{
    public static AgentTeamException Invalid(string field) =>
        new(AgentTeamError.InvalidField, $"Invalid Agent team field: {field}.");

    public static void Identifier(string? value, string field)
    {
        if (string.IsNullOrWhiteSpace(value) || value != value.Trim() ||
            value.Length > 512 || value.Any(char.IsControl))
        {
            throw Invalid(field);
        }
    }

    public static void Text(string? value, string field, int maximumLength)
    {
        if (string.IsNullOrWhiteSpace(value) || value != value.Trim() ||
            value.Length > maximumLength || value.Contains('\0'))
        {
            throw Invalid(field);
        }
    }

    public static void OptionalText(string? value, string field, int maximumLength)
    {
        if (value is null)
        {
            return;
        }

        if (value.Length > maximumLength || value.Contains('\0'))
        {
            throw Invalid(field);
        }
    }

    public static void Identifiers(
        IReadOnlyList<string> values,
        string field,
        int maximumCount)
    {
        if (values.Count > maximumCount || values.Distinct(StringComparer.Ordinal).Count() != values.Count)
        {
            throw Invalid(field);
        }

        foreach (var value in values)
        {
            Identifier(value, field);
        }
    }

    public static void Timestamps(long createdAtUnixMs, long updatedAtUnixMs)
    {
        if (createdAtUnixMs < 0 || updatedAtUnixMs < createdAtUnixMs)
        {
            throw Invalid("timestamps");
        }
    }
}
