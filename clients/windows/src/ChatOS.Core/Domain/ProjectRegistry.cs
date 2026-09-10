using System.Text.Json.Serialization;

namespace ChatOS.Core.Domain;

public enum LocalProjectStatus { Active, Archived, Removed }
public enum ProjectRegistryError { InvalidField, NotFound, RevisionConflict, Removed, ImportSourceConflict }

public sealed class ProjectRegistryException(ProjectRegistryError code, string message) : Exception(message)
{
    public ProjectRegistryError Code { get; } = code;
}

// Host metadata only. Git facts, conversation data and plugin business data are not registry fields.
public sealed record LocalProjectDraft(
    string Name, string WorkspaceId, string RelativeRoot = "", string Description = "")
{
    public void Validate()
    {
        ProjectRegistryValidation.Identifier(Name, nameof(Name));
        ProjectRegistryValidation.RouteIdentifier(WorkspaceId, nameof(WorkspaceId));
        ProjectRegistryValidation.RelativeRoot(RelativeRoot);
        if (Description is null || Description.Contains('\0'))
            throw ProjectRegistryValidation.Invalid(nameof(Description));
    }
}

public sealed record LocalProjectRecord(
    string Id, string OwnerUserId, LocalProjectDraft Draft,
    long Revision, LocalProjectStatus Status, long CreatedAtUnixMs, long UpdatedAtUnixMs)
{
    public void Validate()
    {
        ProjectRegistryValidation.Identifier(Id, nameof(Id));
        ProjectRegistryValidation.Identifier(OwnerUserId, nameof(OwnerUserId));
        if (Draft is null) throw ProjectRegistryValidation.Invalid(nameof(Draft));
        Draft.Validate();
        if (Revision <= 0 || Revision == long.MaxValue) throw ProjectRegistryValidation.Invalid(nameof(Revision));
        if (!Enum.IsDefined(Status)) throw ProjectRegistryValidation.Invalid(nameof(Status));
        if (CreatedAtUnixMs < 0 || UpdatedAtUnixMs < CreatedAtUnixMs)
            throw ProjectRegistryValidation.Invalid("timestamps");
    }
}

public sealed record ProjectRegistryImportResult(IReadOnlyList<string> InsertedIds, IReadOnlyList<string> SkippedIds);

public static class ProjectRegistryValidation
{
    public static void RouteIdentifier(string value, string field)
    {
        Identifier(value, field);
        if (value.Contains('/') || value.Contains('\\') || value is "." or "..") throw Invalid(field);
    }

    public static ProjectRegistryException Invalid(string field) =>
        new(ProjectRegistryError.InvalidField, $"Invalid project field: {field}");

    public static void Identifier(string value, string field)
    {
        if (string.IsNullOrEmpty(value) || value != value.Trim() || value.Any(char.IsControl))
            throw Invalid(field);
    }

    // Syntax only, not authorization. The connector must resolve symlinks and check grants at use time.
    public static void RelativeRoot(string value)
    {
        if (value is null || value != value.Trim() || value.Contains('\\') || value.Contains(':') || value.Any(char.IsControl) ||
            (value.Length > 0 && value.Split('/').Any(segment => segment is "" or "." or "..")))
            throw Invalid(nameof(RelativeRoot));
    }
}

// Untrusted request DTO until the server authenticates and freezes the execution target.
// No owner identity or absolute paths. Explicit wire keys match the macOS snapshot contract.
public sealed record ProjectContextSnapshot(
    [property: JsonPropertyName("schemaVersion")] int SchemaVersion,
    [property: JsonPropertyName("projectId")] string ProjectId,
    [property: JsonPropertyName("projectName")] string ProjectName,
    [property: JsonPropertyName("projectRevision")] long ProjectRevision,
    [property: JsonPropertyName("executionTarget")] ProjectContextExecutionTarget ExecutionTarget)
{
    public static ProjectContextSnapshot FromRecord(LocalProjectRecord record, string deviceId)
    {
        record.Validate();
        ProjectRegistryValidation.RouteIdentifier(deviceId, nameof(deviceId));
        if (record.Status != LocalProjectStatus.Active) throw ProjectRegistryValidation.Invalid("status");
        return new(1, record.Id, record.Draft.Name, record.Revision,
            new(deviceId, record.Draft.WorkspaceId, record.Draft.RelativeRoot));
    }
}

public sealed record ProjectContextExecutionTarget(
    [property: JsonPropertyName("deviceId")] string DeviceId,
    [property: JsonPropertyName("workspaceId")] string WorkspaceId,
    [property: JsonPropertyName("relativeRoot")] string RelativeRoot);
