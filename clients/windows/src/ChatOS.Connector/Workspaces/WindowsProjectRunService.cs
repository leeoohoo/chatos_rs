using ChatOS.Connector.Runtime;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Workspaces;

public sealed class WindowsProjectRunService(
    IProjectRegistry projects,
    ConnectorRuntimeContext runtime,
    WindowsProjectRunSettingsStore settings) : IProjectRunService, IDisposable
{
    private sealed record ProjectRoot(string OwnerUserId, string AbsolutePath);

    private readonly object _gate = new();
    private readonly Dictionary<string, WindowsProjectRunAnalysis> _analyses = new(StringComparer.Ordinal);
    private readonly Dictionary<string, WindowsProjectRunProcess> _processes = new(StringComparer.Ordinal);

    public async Task<ProjectRunCatalog> FetchCatalogAsync(
        string projectId,
        CancellationToken cancellationToken = default) =>
        await CatalogAsync(projectId, force: false, cancellationToken).ConfigureAwait(false);

    public async Task<ProjectRunCatalog> AnalyzeAsync(
        string projectId,
        CancellationToken cancellationToken = default) =>
        await CatalogAsync(projectId, force: true, cancellationToken).ConfigureAwait(false);

    public Task<ProjectRunState> FetchStateAsync(
        string projectId,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        ProjectRunInstance[] instances;
        lock (_gate)
        {
            instances = _processes.Values
                .Where(value => value.ProjectId == projectId)
                .Select(value => value.Snapshot())
                .OrderByDescending(value => value.StartedAt)
                .ToArray();
        }
        var running = instances.Any(value => value.IsRunning);
        return Task.FromResult(new ProjectRunState(
            projectId,
            running ? "running" : "idle",
            false,
            running,
            instances));
    }

    public async Task<ProjectRunEnvironment> FetchEnvironmentAsync(
        string projectId,
        CancellationToken cancellationToken = default)
    {
        var root = await ResolveProjectAsync(projectId, cancellationToken).ConfigureAwait(false);
        var analysis = Analysis(projectId, root, force: false);
        var saved = await settings.LoadAsync(root.OwnerUserId, projectId, cancellationToken)
            .ConfigureAwait(false);
        return Environment(analysis, saved);
    }

    public async Task<ProjectRunEnvironment> UpdateEnvironmentAsync(
        string projectId,
        IReadOnlyDictionary<string, string> selectedToolchains,
        IReadOnlyDictionary<string, ProjectRunCustomToolchain> customToolchains,
        IReadOnlyDictionary<string, string> environmentVariables,
        CancellationToken cancellationToken = default)
    {
        var root = await ResolveProjectAsync(projectId, cancellationToken).ConfigureAwait(false);
        var current = await settings.LoadAsync(root.OwnerUserId, projectId, cancellationToken)
            .ConfigureAwait(false);
        var updated = current with
        {
            SelectedToolchains = Clean(selectedToolchains),
            CustomToolchains = new Dictionary<string, ProjectRunCustomToolchain>(
                customToolchains,
                StringComparer.Ordinal),
            EnvironmentVariables = Clean(environmentVariables),
        };
        await settings.SaveAsync(root.OwnerUserId, projectId, updated, cancellationToken)
            .ConfigureAwait(false);
        return Environment(Analysis(projectId, root, force: false), updated);
    }

    public async Task<ProjectRunCatalog> SetDefaultTargetAsync(
        string projectId,
        string targetId,
        CancellationToken cancellationToken = default)
    {
        var root = await ResolveProjectAsync(projectId, cancellationToken).ConfigureAwait(false);
        var analysis = Analysis(projectId, root, force: false);
        if (!analysis.Targets.Any(value => value.Id == targetId))
        {
            throw new KeyNotFoundException("The project run target does not exist.");
        }
        var current = await settings.LoadAsync(root.OwnerUserId, projectId, cancellationToken)
            .ConfigureAwait(false);
        await settings.SaveAsync(
            root.OwnerUserId,
            projectId,
            current with { DefaultTargetId = targetId },
            cancellationToken).ConfigureAwait(false);
        return ToCatalog(projectId, analysis, targetId);
    }

    public async Task StartAsync(
        string projectId,
        string targetId,
        CancellationToken cancellationToken = default)
    {
        var root = await ResolveProjectAsync(projectId, cancellationToken).ConfigureAwait(false);
        var analysis = Analysis(projectId, root, force: false);
        var target = analysis.Targets.FirstOrDefault(value => value.Id == targetId)
            ?? throw new KeyNotFoundException("The project run target does not exist.");
        var saved = await settings.LoadAsync(root.OwnerUserId, projectId, cancellationToken)
            .ConfigureAwait(false);
        var environment = Environment(analysis, saved, targetId);
        if (environment.ValidationIssues.Count > 0)
        {
            throw new InvalidOperationException(environment.ValidationIssues[0].Message);
        }
        var variables = LaunchEnvironment(analysis, saved);
        var process = new WindowsProjectRunProcess(
            $"project-run-{Guid.NewGuid():N}",
            projectId,
            target,
            variables);
        cancellationToken.ThrowIfCancellationRequested();
        process.Start();
        lock (_gate) _processes.Add(process.Id, process);
    }

    public Task StopAsync(
        string instanceId,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        Process(instanceId).Stop();
        return Task.CompletedTask;
    }

    public Task DeleteAsync(
        string instanceId,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        WindowsProjectRunProcess process;
        lock (_gate)
        {
            if (!_processes.Remove(instanceId, out var removed))
            {
                throw new KeyNotFoundException("The project run instance does not exist.");
            }
            process = removed;
        }
        process.Dispose();
        return Task.CompletedTask;
    }

    public void Dispose()
    {
        WindowsProjectRunProcess[] values;
        lock (_gate)
        {
            values = _processes.Values.ToArray();
            _processes.Clear();
        }
        foreach (var process in values) process.Dispose();
    }

    private async Task<ProjectRunCatalog> CatalogAsync(
        string projectId,
        bool force,
        CancellationToken cancellationToken)
    {
        var root = await ResolveProjectAsync(projectId, cancellationToken).ConfigureAwait(false);
        var analysis = Analysis(projectId, root, force);
        var saved = await settings.LoadAsync(root.OwnerUserId, projectId, cancellationToken)
            .ConfigureAwait(false);
        var selected = saved.DefaultTargetId is { } id && analysis.Targets.Any(value => value.Id == id)
            ? id
            : analysis.Targets.FirstOrDefault()?.Id;
        return ToCatalog(projectId, analysis, selected);
    }

    private WindowsProjectRunAnalysis Analysis(string projectId, ProjectRoot root, bool force)
    {
        var key = $"{root.OwnerUserId}\0{projectId}\0{root.AbsolutePath}";
        lock (_gate)
        {
            if (!force && _analyses.TryGetValue(key, out var cached)) return cached;
        }
        var analysis = WindowsProjectRunAnalyzer.Analyze(root.AbsolutePath);
        lock (_gate) _analyses[key] = analysis;
        return analysis;
    }

    private async Task<ProjectRoot> ResolveProjectAsync(
        string projectId,
        CancellationToken cancellationToken)
    {
        await runtime.InitializeAsync(cancellationToken).ConfigureAwait(false);
        var state = runtime.Snapshot.State
            ?? throw new InvalidOperationException("The local connector is not configured.");
        var project = await projects.GetAsync(state.User.Id, projectId, cancellationToken)
            .ConfigureAwait(false);
        if (project is null || project.Status != LocalProjectStatus.Active)
        {
            throw new KeyNotFoundException("The local project does not exist.");
        }
        var workspace = state.Workspaces.FirstOrDefault(value => value.Id == project.Draft.WorkspaceId)
            ?? throw new InvalidOperationException("The project workspace is unavailable.");
        var relative = project.Draft.RelativeRoot.Length == 0 ? "." : project.Draft.RelativeRoot;
        var absolute = new WorkspacePathGuard(workspace.AbsoluteRoot).ResolveExisting(relative);
        if (!Directory.Exists(absolute))
        {
            throw new InvalidOperationException("The project directory is unavailable.");
        }
        return new(state.User.Id, absolute);
    }

    private WindowsProjectRunProcess Process(string instanceId)
    {
        lock (_gate)
        {
            return _processes.TryGetValue(instanceId, out var process)
                ? process
                : throw new KeyNotFoundException("The project run instance does not exist.");
        }
    }

    private static ProjectRunCatalog ToCatalog(
        string projectId,
        WindowsProjectRunAnalysis analysis,
        string? selected) => new(
        projectId,
        analysis.Targets.Count == 0 ? "empty" : "ready",
        selected,
        analysis.Targets.Select(value => value with { IsDefault = value.Id == selected }).ToArray(),
        null);

    private static ProjectRunEnvironment Environment(
        WindowsProjectRunAnalysis analysis,
        WindowsProjectRunSettings saved,
        string? targetId = null)
    {
        var issues = new List<ProjectRunValidationIssue>();
        var preferredTargetId = targetId ?? saved.DefaultTargetId;
        var selectedTargetId = preferredTargetId is { } preferred &&
            analysis.Targets.Any(value => value.Id == preferred)
            ? preferred
            : analysis.Targets.FirstOrDefault()?.Id;
        var targets = analysis.Targets.Where(value => value.Id == selectedTargetId);
        foreach (var target in targets)
        {
            foreach (var kind in target.RequiredToolchains)
            {
                if (Available(kind, analysis, saved)) continue;
                issues.Add(new ProjectRunValidationIssue(
                    "error",
                    $"No available {kind} toolchain was found.",
                    target.Id,
                    target.Label,
                    null,
                    $"Install {kind} or select an executable in project run settings."));
            }
        }
        return new ProjectRunEnvironment(
            analysis.Toolchains,
            analysis.ConfigurationFiles,
            issues.DistinctBy(value => value.Id).ToArray(),
            saved.SelectedToolchains,
            saved.CustomToolchains,
            saved.EnvironmentVariables,
            false);
    }

    private static bool Available(
        string kind,
        WindowsProjectRunAnalysis analysis,
        WindowsProjectRunSettings saved)
    {
        if (saved.SelectedToolchains.TryGetValue(kind, out var selected) && selected.Length > 0)
        {
            return analysis.Toolchains.TryGetValue(kind, out var options) &&
                options.Any(value => value.Id == selected && File.Exists(value.Path));
        }
        if (saved.CustomToolchains.TryGetValue(kind, out var custom) && File.Exists(custom.Path))
        {
            return true;
        }
        return analysis.Toolchains.TryGetValue(kind, out var discovered) &&
            discovered.Any(value => File.Exists(value.Path));
    }

    private static Dictionary<string, string> LaunchEnvironment(
        WindowsProjectRunAnalysis analysis,
        WindowsProjectRunSettings saved)
    {
        var environment = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        foreach (System.Collections.DictionaryEntry value in Environment.GetEnvironmentVariables())
        {
            if (value.Key is string key && value.Value is string item) environment[key] = item;
        }
        foreach (var pair in saved.EnvironmentVariables) environment[pair.Key] = pair.Value;
        var executables = new List<string>();
        foreach (var kind in analysis.Toolchains.Keys)
        {
            if (saved.SelectedToolchains.TryGetValue(kind, out var selected) &&
                analysis.Toolchains[kind].FirstOrDefault(value => value.Id == selected) is { } option)
            {
                executables.Add(option.Path);
            }
            else if (saved.CustomToolchains.TryGetValue(kind, out var custom))
            {
                executables.Add(custom.Path);
            }
        }
        var directories = executables.Select(Path.GetDirectoryName)
            .OfType<string>()
            .Distinct(StringComparer.OrdinalIgnoreCase)
            .ToArray();
        if (directories.Length > 0)
        {
            environment["PATH"] = string.Join(
                Path.PathSeparator,
                directories.Append(environment.GetValueOrDefault("PATH") ?? string.Empty));
        }
        return environment;
    }

    private static IReadOnlyDictionary<string, string> Clean(
        IReadOnlyDictionary<string, string> values) => values
        .Where(pair => !string.IsNullOrWhiteSpace(pair.Key))
        .ToDictionary(
            pair => pair.Key.Trim(),
            pair => pair.Value?.Trim() ?? string.Empty,
            StringComparer.Ordinal);
}
