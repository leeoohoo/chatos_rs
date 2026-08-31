using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Projects;

public sealed class ProjectRunService : IProjectRunService
{
    private readonly ChatOSApiClient _client;

    public ProjectRunService(ChatOSApiClient client)
    {
        _client = client;
    }

    public async Task<ProjectRunCatalog> FetchCatalogAsync(
        string projectId,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.GetAsync<ProjectRunCatalogDto>(
            $"projects/{Path(projectId)}/run/catalog",
            cancellationToken).ConfigureAwait(false);
        return response.ToDomain(projectId);
    }

    public async Task<ProjectRunCatalog> AnalyzeAsync(
        string projectId,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.PostAsync<ProjectRunCatalogDto>(
            $"projects/{Path(projectId)}/run/analyze",
            cancellationToken: cancellationToken).ConfigureAwait(false);
        return response.ToDomain(projectId);
    }

    public async Task<ProjectRunState> FetchStateAsync(
        string projectId,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.GetAsync<ProjectRunStateDto>(
            $"projects/{Path(projectId)}/run/state",
            cancellationToken).ConfigureAwait(false);
        return response.ToDomain(projectId);
    }

    public async Task<ProjectRunEnvironment> FetchEnvironmentAsync(
        string projectId,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.GetAsync<ProjectRunEnvironmentDto>(
            $"projects/{Path(projectId)}/run/environment",
            cancellationToken).ConfigureAwait(false);
        return response.ToDomain();
    }

    public async Task<ProjectRunEnvironment> UpdateEnvironmentAsync(
        string projectId,
        IReadOnlyDictionary<string, string> selectedToolchains,
        IReadOnlyDictionary<string, ProjectRunCustomToolchain> customToolchains,
        IReadOnlyDictionary<string, string> environmentVariables,
        CancellationToken cancellationToken = default)
    {
        var custom = customToolchains.ToDictionary(
            static pair => pair.Key,
            static pair => new ProjectRunCustomToolchainRequestDto(
                pair.Value.Kind,
                pair.Value.Label,
                pair.Value.Path),
            StringComparer.Ordinal);
        var response = await _client.PutAsync<ProjectRunEnvironmentDto>(
            $"projects/{Path(projectId)}/run/environment",
            new ProjectRunEnvironmentUpdateRequestDto(
                selectedToolchains,
                custom,
                environmentVariables),
            cancellationToken).ConfigureAwait(false);
        return response.ToDomain();
    }

    public async Task<ProjectRunCatalog> SetDefaultTargetAsync(
        string projectId,
        string targetId,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.PostAsync<ProjectRunCatalogDto>(
            $"projects/{Path(projectId)}/run/default",
            new ProjectRunDefaultTargetRequestDto(targetId),
            cancellationToken).ConfigureAwait(false);
        return response.ToDomain(projectId);
    }

    public async Task StartAsync(
        string projectId,
        string targetId,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.PostAsync<ProjectRunMutationDto>(
            $"projects/{Path(projectId)}/run/execute",
            new ProjectRunStartRequestDto(targetId, true),
            cancellationToken).ConfigureAwait(false);
        EnsureSucceeded(response, "项目启动请求未被接受。");
    }

    public async Task StopAsync(
        string instanceId,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.PostAsync<ProjectRunMutationDto>(
            $"terminals/{Path(instanceId)}/interrupt",
            cancellationToken: cancellationToken).ConfigureAwait(false);
        EnsureSucceeded(response, "停止请求未被接受。");
    }

    public async Task DeleteAsync(
        string instanceId,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.DeleteAsync<ProjectRunMutationDto>(
            $"terminals/{Path(instanceId)}",
            cancellationToken).ConfigureAwait(false);
        EnsureSucceeded(response, "删除运行实例失败。");
    }

    private static void EnsureSucceeded(ProjectRunMutationDto response, string fallbackMessage)
    {
        if (response.Success == false)
        {
            throw new ChatOSApiException(string.IsNullOrWhiteSpace(response.Status)
                ? fallbackMessage
                : response.Status);
        }
    }

    private static string Path(string value) => Uri.EscapeDataString(value);
}

internal sealed record ProjectRunCatalogDto
{
    [JsonPropertyName("project_id")]
    public string? ProjectId { get; init; }

    [JsonPropertyName("status")]
    public string? Status { get; init; }

    [JsonPropertyName("default_target_id")]
    public string? DefaultTargetId { get; init; }

    [JsonPropertyName("targets")]
    public IReadOnlyList<ProjectRunTargetDto> Targets { get; init; } = Array.Empty<ProjectRunTargetDto>();

    [JsonPropertyName("error_message")]
    public string? ErrorMessage { get; init; }

    public ProjectRunCatalog ToDomain(string fallbackProjectId) => new(
        ProjectId ?? fallbackProjectId,
        Status ?? "unknown",
        DefaultTargetId,
        Targets.Select(static value => value.ToDomain()).ToArray(),
        ErrorMessage);
}

internal sealed record ProjectRunTargetDto
{
    [JsonPropertyName("id")]
    public required string Id { get; init; }

    [JsonPropertyName("label")]
    public string? Label { get; init; }

    [JsonPropertyName("kind")]
    public string? Kind { get; init; }

    [JsonPropertyName("language")]
    public string? Language { get; init; }

    [JsonPropertyName("cwd")]
    public string? WorkingDirectory { get; init; }

    [JsonPropertyName("command")]
    public string? Command { get; init; }

    [JsonPropertyName("source")]
    public string? Source { get; init; }

    [JsonPropertyName("is_default")]
    public bool? IsDefault { get; init; }

    [JsonPropertyName("entrypoint")]
    public string? Entrypoint { get; init; }

    [JsonPropertyName("manifest_path")]
    public string? ManifestPath { get; init; }

    [JsonPropertyName("required_toolchains")]
    public IReadOnlyList<string> RequiredToolchains { get; init; } = Array.Empty<string>();

    public ProjectRunTarget ToDomain() => new(
        Id,
        Label ?? Id,
        Kind ?? "custom",
        Language,
        WorkingDirectory ?? string.Empty,
        Command ?? string.Empty,
        Source ?? "unknown",
        IsDefault ?? false,
        Entrypoint,
        ManifestPath,
        RequiredToolchains);
}

internal sealed record ProjectRunStateDto
{
    [JsonPropertyName("project_id")]
    public string? ProjectId { get; init; }

    [JsonPropertyName("status")]
    public string? Status { get; init; }

    [JsonPropertyName("busy")]
    public bool? Busy { get; init; }

    [JsonPropertyName("running")]
    public bool? Running { get; init; }

    [JsonPropertyName("instances")]
    public IReadOnlyList<ProjectRunInstanceDto> Instances { get; init; } = Array.Empty<ProjectRunInstanceDto>();

    public ProjectRunState ToDomain(string fallbackProjectId) => new(
        ProjectId ?? fallbackProjectId,
        Status ?? "idle",
        Busy ?? false,
        Running ?? false,
        Instances.Select(static value => value.ToDomain()).Where(static value => value is not null).Cast<ProjectRunInstance>().ToArray());
}

internal sealed record ProjectRunInstanceDto
{
    [JsonPropertyName("terminal_id")]
    public string? TerminalId { get; init; }

    [JsonPropertyName("terminal_name")]
    public string? TerminalName { get; init; }

    [JsonPropertyName("cwd")]
    public string? WorkingDirectory { get; init; }

    [JsonPropertyName("status")]
    public string? Status { get; init; }

    [JsonPropertyName("busy")]
    public bool? Busy { get; init; }

    [JsonPropertyName("running")]
    public bool? Running { get; init; }

    [JsonPropertyName("log")]
    public string? Log { get; init; }

    [JsonPropertyName("started_at")]
    public string? StartedAt { get; init; }

    [JsonPropertyName("exit_code")]
    public int? ExitCode { get; init; }

    public ProjectRunInstance? ToDomain()
    {
        if (string.IsNullOrWhiteSpace(TerminalId))
        {
            return null;
        }

        return new ProjectRunInstance(
            TerminalId,
            TerminalName ?? TerminalId,
            WorkingDirectory,
            Status ?? "unknown",
            Busy ?? false,
            Running ?? false,
            Log,
            DateTimeOffset.TryParse(StartedAt, out var startedAt) ? startedAt : null,
            ExitCode);
    }
}

internal sealed record ProjectRunEnvironmentDto
{
    [JsonPropertyName("options_by_kind")]
    public IReadOnlyDictionary<string, IReadOnlyList<ProjectRunToolchainOptionDto>> OptionsByKind { get; init; }
        = new Dictionary<string, IReadOnlyList<ProjectRunToolchainOptionDto>>();

    [JsonPropertyName("config_files")]
    public IReadOnlyList<ProjectRunConfigurationFileDto> ConfigurationFiles { get; init; }
        = Array.Empty<ProjectRunConfigurationFileDto>();

    [JsonPropertyName("validation_issues")]
    public IReadOnlyList<ProjectRunValidationIssueDto> ValidationIssues { get; init; }
        = Array.Empty<ProjectRunValidationIssueDto>();

    [JsonPropertyName("selected_toolchains")]
    public IReadOnlyDictionary<string, string> SelectedToolchains { get; init; }
        = new Dictionary<string, string>();

    [JsonPropertyName("custom_toolchains")]
    public IReadOnlyDictionary<string, ProjectRunCustomToolchainDto> CustomToolchains { get; init; }
        = new Dictionary<string, ProjectRunCustomToolchainDto>();

    [JsonPropertyName("env_vars")]
    public IReadOnlyDictionary<string, string> EnvironmentVariables { get; init; }
        = new Dictionary<string, string>();

    [JsonPropertyName("terminal_ui_enabled")]
    public bool? TerminalUiEnabled { get; init; }

    public ProjectRunEnvironment ToDomain() => new(
        OptionsByKind.ToDictionary(
            static pair => pair.Key,
            static pair => (IReadOnlyList<ProjectRunToolchainOption>)pair.Value.Select(static value => value.ToDomain()).ToArray(),
            StringComparer.Ordinal),
        ConfigurationFiles.Select(static value => value.ToDomain()).ToArray(),
        ValidationIssues.Select(static value => value.ToDomain()).ToArray(),
        new Dictionary<string, string>(SelectedToolchains, StringComparer.Ordinal),
        CustomToolchains.ToDictionary(
            static pair => pair.Key,
            static pair => pair.Value.ToDomain(),
            StringComparer.Ordinal),
        new Dictionary<string, string>(EnvironmentVariables, StringComparer.Ordinal),
        TerminalUiEnabled ?? false);
}

internal sealed record ProjectRunToolchainOptionDto
{
    [JsonPropertyName("id")]
    public required string Id { get; init; }

    [JsonPropertyName("kind")]
    public string? Kind { get; init; }

    [JsonPropertyName("label")]
    public string? Label { get; init; }

    [JsonPropertyName("version")]
    public string? Version { get; init; }

    [JsonPropertyName("path")]
    public string? Path { get; init; }

    [JsonPropertyName("source")]
    public string? Source { get; init; }

    [JsonPropertyName("is_default")]
    public bool? IsDefault { get; init; }

    public ProjectRunToolchainOption ToDomain() => new(
        Id,
        Kind ?? string.Empty,
        Label ?? Id,
        Version,
        Path ?? string.Empty,
        Source ?? "auto",
        IsDefault ?? false);
}

internal sealed record ProjectRunConfigurationFileDto
{
    [JsonPropertyName("kind")]
    public string? Kind { get; init; }

    [JsonPropertyName("label")]
    public string? Label { get; init; }

    [JsonPropertyName("path")]
    public string? Path { get; init; }

    [JsonPropertyName("preview")]
    public string? Preview { get; init; }

    [JsonPropertyName("source")]
    public string? Source { get; init; }

    public ProjectRunConfigurationFile ToDomain() => new(
        Kind ?? "config",
        Label ?? "配置文件",
        Path ?? string.Empty,
        Preview,
        Source ?? "project");
}

internal sealed record ProjectRunValidationIssueDto
{
    [JsonPropertyName("kind")]
    public string? Kind { get; init; }

    [JsonPropertyName("message")]
    public string? Message { get; init; }

    [JsonPropertyName("target_id")]
    public string? TargetId { get; init; }

    [JsonPropertyName("target_label")]
    public string? TargetLabel { get; init; }

    [JsonPropertyName("path")]
    public string? Path { get; init; }

    [JsonPropertyName("hint")]
    public string? Hint { get; init; }

    public ProjectRunValidationIssue ToDomain() => new(
        Kind ?? "validation",
        Message ?? "未知问题",
        TargetId,
        TargetLabel,
        Path,
        Hint);
}

internal sealed record ProjectRunCustomToolchainDto
{
    [JsonPropertyName("kind")]
    public string? Kind { get; init; }

    [JsonPropertyName("label")]
    public string? Label { get; init; }

    [JsonPropertyName("path")]
    public string? Path { get; init; }

    public ProjectRunCustomToolchain ToDomain() => new(
        Kind ?? string.Empty,
        Label ?? string.Empty,
        Path ?? string.Empty);
}

internal sealed record ProjectRunMutationDto(
    [property: JsonPropertyName("success")] bool? Success,
    [property: JsonPropertyName("status")] string? Status);

internal sealed record ProjectRunDefaultTargetRequestDto(
    [property: JsonPropertyName("target_id")] string TargetId);

internal sealed record ProjectRunStartRequestDto(
    [property: JsonPropertyName("target_id")] string TargetId,
    [property: JsonPropertyName("create_if_missing")] bool CreateIfMissing);

internal sealed record ProjectRunCustomToolchainRequestDto(
    [property: JsonPropertyName("kind")] string Kind,
    [property: JsonPropertyName("label")] string Label,
    [property: JsonPropertyName("path")] string Path);

internal sealed record ProjectRunEnvironmentUpdateRequestDto(
    [property: JsonPropertyName("selected_toolchains")] IReadOnlyDictionary<string, string> SelectedToolchains,
    [property: JsonPropertyName("custom_toolchains")] IReadOnlyDictionary<string, ProjectRunCustomToolchainRequestDto> CustomToolchains,
    [property: JsonPropertyName("env_vars")] IReadOnlyDictionary<string, string> EnvironmentVariables);
