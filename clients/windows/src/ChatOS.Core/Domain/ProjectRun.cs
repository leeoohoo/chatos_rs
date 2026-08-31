namespace ChatOS.Core.Domain;

public sealed record ProjectRunTarget(
    string Id,
    string Label,
    string Kind,
    string? Language,
    string WorkingDirectory,
    string Command,
    string Source,
    bool IsDefault,
    string? Entrypoint,
    string? ManifestPath,
    IReadOnlyList<string> RequiredToolchains);

public sealed record ProjectRunCatalog(
    string ProjectId,
    string Status,
    string? DefaultTargetId,
    IReadOnlyList<ProjectRunTarget> Targets,
    string? ErrorMessage);

public sealed record ProjectRunInstance(
    string Id,
    string Name,
    string? WorkingDirectory,
    string Status,
    bool IsBusy,
    bool IsRunning,
    string? Log,
    DateTimeOffset? StartedAt,
    int? ExitCode);

public sealed record ProjectRunState(
    string ProjectId,
    string Status,
    bool IsBusy,
    bool IsRunning,
    IReadOnlyList<ProjectRunInstance> Instances);

public sealed record ProjectRunValidationIssue(
    string Kind,
    string Message,
    string? TargetId,
    string? TargetLabel,
    string? Path,
    string? Hint)
{
    public string Id => $"{Kind}:{TargetId ?? string.Empty}:{Path ?? Message}";
}

public sealed record ProjectRunToolchainOption(
    string Id,
    string Kind,
    string Label,
    string? Version,
    string Path,
    string Source,
    bool IsDefault);

public sealed record ProjectRunConfigurationFile(
    string Kind,
    string Label,
    string Path,
    string? Preview,
    string Source)
{
    public string Id => $"{Kind}:{Path}";
}

public sealed record ProjectRunCustomToolchain(
    string Kind,
    string Label,
    string Path);

public sealed record ProjectRunEnvironment(
    IReadOnlyDictionary<string, IReadOnlyList<ProjectRunToolchainOption>> ToolchainOptions,
    IReadOnlyList<ProjectRunConfigurationFile> ConfigurationFiles,
    IReadOnlyList<ProjectRunValidationIssue> ValidationIssues,
    IReadOnlyDictionary<string, string> SelectedToolchains,
    IReadOnlyDictionary<string, ProjectRunCustomToolchain> CustomToolchains,
    IReadOnlyDictionary<string, string> EnvironmentVariables,
    bool TerminalUiEnabled)
{
    public static ProjectRunEnvironment Empty { get; } = new(
        new Dictionary<string, IReadOnlyList<ProjectRunToolchainOption>>(),
        Array.Empty<ProjectRunConfigurationFile>(),
        Array.Empty<ProjectRunValidationIssue>(),
        new Dictionary<string, string>(),
        new Dictionary<string, ProjectRunCustomToolchain>(),
        new Dictionary<string, string>(),
        false);
}
