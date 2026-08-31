using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Projects;

public sealed partial class ProjectRunViewModel : ObservableObject, IDisposable
{
    private readonly IProjectRunService _service;
    private readonly IUiDispatcher _dispatcher;
    private readonly LocalizationViewModel? _localization;
    private CancellationTokenSource? _sessionCancellation;
    private IReadOnlyDictionary<string, ProjectRunCustomToolchain> _customToolchains
        = new Dictionary<string, ProjectRunCustomToolchain>();
    private long _sessionGeneration;
    private long _loadGeneration;
    private long _mutationGeneration;
    private long _stateRequestGeneration;

    public ProjectRunViewModel(
        IProjectRunService service,
        IUiDispatcher dispatcher,
        LocalizationViewModel? localization = null)
    {
        _service = service;
        _dispatcher = dispatcher;
        _localization = localization;
        if (_localization is not null) _localization.PropertyChanged += OnLocalizationChanged;
        Targets.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasTargets));
        Instances.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasInstances));
        ValidationIssues.CollectionChanged += (_, _) =>
        {
            OnPropertyChanged(nameof(HasValidationIssues));
            OnPropertyChanged(nameof(CanStart));
        };
        Toolchains.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasToolchains));
        EnvironmentVariables.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasEnvironmentVariables));
        ConfigurationFiles.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasConfigurationFiles));
    }

    public ObservableCollection<ProjectRunTarget> Targets { get; } = [];

    public ObservableCollection<ProjectRunInstance> Instances { get; } = [];

    public ObservableCollection<ProjectRunValidationIssue> ValidationIssues { get; } = [];

    public ObservableCollection<ProjectRunToolchainSelectionViewModel> Toolchains { get; } = [];

    public ObservableCollection<ProjectRunEnvironmentVariableViewModel> EnvironmentVariables { get; } = [];

    public ObservableCollection<ProjectRunConfigurationFile> ConfigurationFiles { get; } = [];

    public bool HasTargets => Targets.Count > 0;

    public bool HasInstances => Instances.Count > 0;

    public bool HasValidationIssues => ValidationIssues.Count > 0;

    public bool HasToolchains => Toolchains.Count > 0;

    public bool HasEnvironmentVariables => EnvironmentVariables.Count > 0;

    public bool HasConfigurationFiles => ConfigurationFiles.Count > 0;

    public bool CanStart => SelectedTarget is not null && !HasValidationIssues && !IsMutating;

    public bool CanMutate => !IsMutating;

    public string RunStatusLabel => LocalizeStatus(RunStatus);

    [ObservableProperty]
    private string? _projectId;

    [ObservableProperty]
    private string _projectName = string.Empty;

    [ObservableProperty]
    private string? _projectRoot;

    [ObservableProperty]
    private string _runStatus = "idle";

    [ObservableProperty]
    private bool _isRunning;

    [ObservableProperty]
    private bool _isBusy;

    [ObservableProperty]
    private bool _isLoading;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanStart))]
    [NotifyPropertyChangedFor(nameof(CanMutate))]
    private bool _isMutating;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanStart))]
    private ProjectRunTarget? _selectedTarget;

    [ObservableProperty]
    private bool _terminalUiEnabled;

    [ObservableProperty]
    private string? _catalogErrorMessage;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private string? _noticeMessage;

    partial void OnRunStatusChanged(string value) => OnPropertyChanged(nameof(RunStatusLabel));

    public async Task OpenAsync(
        WorkspaceProject project,
        CancellationToken cancellationToken = default)
    {
        CancelSession();
        _sessionCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        var token = _sessionCancellation.Token;
        var session = Interlocked.Increment(ref _sessionGeneration);
        await _dispatcher.InvokeAsync(() => Reset(project), token).ConfigureAwait(false);
        await LoadAllInternalAsync(
            project.Id,
            session,
            Interlocked.Increment(ref _loadGeneration),
            token).ConfigureAwait(false);
        _ = MonitorStateSafelyAsync(project.Id, session, token);
    }

    public Task CloseAsync(CancellationToken cancellationToken = default)
    {
        CancelSession();
        return _dispatcher.InvokeAsync(ResetClosed, cancellationToken);
    }

    [RelayCommand]
    private Task RefreshAsync()
    {
        if (!TryGetSession(out var projectId, out var session, out var token) || IsMutating)
        {
            return Task.CompletedTask;
        }

        return LoadAllInternalAsync(
            projectId,
            session,
            Interlocked.Increment(ref _loadGeneration),
            token);
    }

    [RelayCommand]
    private async Task AnalyzeAsync()
    {
        if (!TryBeginMutation(out var projectId, out var session, out var mutation, out var token))
        {
            return;
        }

        try
        {
            var catalog = await _service.AnalyzeAsync(projectId, token).ConfigureAwait(false);
            var environment = await _service.FetchEnvironmentAsync(projectId, token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (!IsCurrentMutation(projectId, session, mutation))
                {
                    return;
                }

                ApplyCatalog(catalog, null);
                ApplyEnvironment(environment);
                NoticeMessage = L("项目运行目标已重新分析。", "Project run targets were analyzed again.");
            }, token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await SetMutationErrorAsync(projectId, session, mutation, exception).ConfigureAwait(false);
        }
        finally
        {
            await FinishMutationAsync(projectId, session, mutation).ConfigureAwait(false);
        }
    }

    [RelayCommand]
    private async Task SelectTargetAsync(ProjectRunTarget? target)
    {
        if (target is null || !TryBeginMutation(out var projectId, out var session, out var mutation, out var token))
        {
            return;
        }

        var targetId = target.Id;
        var previousTargetId = SelectedTarget?.Id;
        SelectedTarget = Targets.FirstOrDefault(value => value.Id == targetId);
        try
        {
            var catalog = await _service.SetDefaultTargetAsync(projectId, targetId, token)
                .ConfigureAwait(false);
            var environment = await _service.FetchEnvironmentAsync(projectId, token)
                .ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (!IsCurrentMutation(projectId, session, mutation))
                {
                    return;
                }

                ApplyCatalog(catalog, targetId);
                ApplyEnvironment(environment);
                NoticeMessage = L("默认运行目标已保存。", "The default run target was saved.");
            }, token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (!IsCurrentMutation(projectId, session, mutation))
                {
                    return;
                }

                SelectedTarget = Targets.FirstOrDefault(value => value.Id == previousTargetId)
                    ?? Targets.FirstOrDefault();
                ErrorMessage = exception.Message;
            }).ConfigureAwait(false);
        }
        finally
        {
            await FinishMutationAsync(projectId, session, mutation).ConfigureAwait(false);
        }
    }

    [RelayCommand]
    private async Task SaveEnvironmentAsync()
    {
        if (!TryBeginMutation(out var projectId, out var session, out var mutation, out var token))
        {
            return;
        }

        var selectedToolchains = Toolchains.ToDictionary(
            static value => value.Kind,
            static value => value.SelectedOptionId ?? string.Empty,
            StringComparer.Ordinal);
        var environmentVariables = EnvironmentVariables
            .Select(static value => (Key: value.Key.Trim(), value.Value))
            .Where(static value => value.Key.Length > 0)
            .GroupBy(static value => value.Key, StringComparer.Ordinal)
            .ToDictionary(
                static group => group.Key,
                static group => group.Last().Value,
                StringComparer.Ordinal);
        try
        {
            var environment = await _service.UpdateEnvironmentAsync(
                projectId,
                selectedToolchains,
                _customToolchains,
                environmentVariables,
                token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (!IsCurrentMutation(projectId, session, mutation))
                {
                    return;
                }

                ApplyEnvironment(environment);
                NoticeMessage = L("工具链和环境变量已保存。", "Toolchains and environment variables were saved.");
            }, token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            // Intentionally do not reapply the last server environment here. The user's
            // unsaved toolchain and variable drafts must survive a failed save.
            await SetMutationErrorAsync(projectId, session, mutation, exception).ConfigureAwait(false);
        }
        finally
        {
            await FinishMutationAsync(projectId, session, mutation).ConfigureAwait(false);
        }
    }

    [RelayCommand]
    private void AddEnvironmentVariable() =>
        EnvironmentVariables.Add(new ProjectRunEnvironmentVariableViewModel());

    [RelayCommand]
    private void RemoveEnvironmentVariable(ProjectRunEnvironmentVariableViewModel? variable)
    {
        if (variable is not null)
        {
            EnvironmentVariables.Remove(variable);
        }
    }

    [RelayCommand]
    private async Task StartAsync()
    {
        var targetId = SelectedTarget?.Id;
        if (targetId is null || HasValidationIssues ||
            !TryBeginMutation(out var projectId, out var session, out var mutation, out var token))
        {
            return;
        }

        try
        {
            await _service.StartAsync(projectId, targetId, token).ConfigureAwait(false);
            await Task.Delay(TimeSpan.FromMilliseconds(450), token).ConfigureAwait(false);
            await RefreshStateForMutationAsync(projectId, session, mutation, token).ConfigureAwait(false);
            await SetMutationNoticeAsync(projectId, session, mutation, L("运行实例已启动。", "The run instance started.")).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await SetMutationErrorAsync(projectId, session, mutation, exception).ConfigureAwait(false);
        }
        finally
        {
            await FinishMutationAsync(projectId, session, mutation).ConfigureAwait(false);
        }
    }

    [RelayCommand]
    private async Task StopAsync(ProjectRunInstance? instance)
    {
        var instanceId = instance?.Id;
        if (instanceId is null ||
            !TryBeginMutation(out var projectId, out var session, out var mutation, out var token))
        {
            return;
        }

        try
        {
            await _service.StopAsync(instanceId, token).ConfigureAwait(false);
            await Task.Delay(TimeSpan.FromMilliseconds(300), token).ConfigureAwait(false);
            await RefreshStateForMutationAsync(projectId, session, mutation, token).ConfigureAwait(false);
            await SetMutationNoticeAsync(projectId, session, mutation, L("已向运行实例发送停止信号。", "A stop signal was sent to the run instance.")).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await SetMutationErrorAsync(projectId, session, mutation, exception).ConfigureAwait(false);
        }
        finally
        {
            await FinishMutationAsync(projectId, session, mutation).ConfigureAwait(false);
        }
    }

    [RelayCommand]
    private async Task DeleteInstanceAsync(ProjectRunInstance? instance)
    {
        var instanceId = instance?.Id;
        if (instanceId is null ||
            !TryBeginMutation(out var projectId, out var session, out var mutation, out var token))
        {
            return;
        }

        try
        {
            await _service.DeleteAsync(instanceId, token).ConfigureAwait(false);
            await RefreshStateForMutationAsync(projectId, session, mutation, token).ConfigureAwait(false);
            await SetMutationNoticeAsync(projectId, session, mutation, L("运行实例已删除。", "The run instance was deleted.")).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await SetMutationErrorAsync(projectId, session, mutation, exception).ConfigureAwait(false);
        }
        finally
        {
            await FinishMutationAsync(projectId, session, mutation).ConfigureAwait(false);
        }
    }

    [RelayCommand]
    private void DismissNotice() => NoticeMessage = null;

    [RelayCommand]
    private void DismissError() => ErrorMessage = null;

    public void Dispose()
    {
        CancelSession();
        if (_localization is not null) _localization.PropertyChanged -= OnLocalizationChanged;
    }

    private async Task LoadAllInternalAsync(
        string projectId,
        long session,
        long load,
        CancellationToken cancellationToken)
    {
        await _dispatcher.InvokeAsync(() =>
        {
            if (!IsCurrentLoad(projectId, session, load))
            {
                return;
            }

            IsLoading = true;
            ErrorMessage = null;
        }, cancellationToken).ConfigureAwait(false);
        var stateRequest = Interlocked.Increment(ref _stateRequestGeneration);
        try
        {
            var catalogTask = _service.FetchCatalogAsync(projectId, cancellationToken);
            var stateTask = _service.FetchStateAsync(projectId, cancellationToken);
            var environmentTask = _service.FetchEnvironmentAsync(projectId, cancellationToken);
            await Task.WhenAll(catalogTask, stateTask, environmentTask).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (!IsCurrentLoad(projectId, session, load))
                {
                    return;
                }

                ApplyCatalog(catalogTask.Result, SelectedTarget?.Id);
                ApplyEnvironment(environmentTask.Result);
                if (stateRequest == _stateRequestGeneration)
                {
                    ApplyState(stateTask.Result);
                }
            }, cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (IsCurrentLoad(projectId, session, load))
                {
                    ErrorMessage = exception.Message;
                }
            }).ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (IsCurrentLoad(projectId, session, load))
                {
                    IsLoading = false;
                }
            }).ConfigureAwait(false);
        }
    }

    private async Task MonitorStateSafelyAsync(
        string projectId,
        long session,
        CancellationToken cancellationToken)
    {
        try
        {
            while (!cancellationToken.IsCancellationRequested)
            {
                await Task.Delay(TimeSpan.FromSeconds(1), cancellationToken).ConfigureAwait(false);
                if (!IsCurrentSession(projectId, session) ||
                    (!IsRunning && !IsBusy && Instances.Count == 0))
                {
                    continue;
                }

                var stateRequest = Interlocked.Increment(ref _stateRequestGeneration);
                try
                {
                    var state = await _service.FetchStateAsync(projectId, cancellationToken)
                        .ConfigureAwait(false);
                    await _dispatcher.InvokeAsync(() =>
                    {
                        if (IsCurrentSession(projectId, session) &&
                            stateRequest == _stateRequestGeneration)
                        {
                            ApplyState(state);
                        }
                    }, cancellationToken).ConfigureAwait(false);
                }
                catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
                {
                    return;
                }
                catch
                {
                    // Polling is opportunistic. Explicit refresh and mutations report errors.
                }
            }
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
    }

    private async Task RefreshStateForMutationAsync(
        string projectId,
        long session,
        long mutation,
        CancellationToken cancellationToken)
    {
        var stateRequest = Interlocked.Increment(ref _stateRequestGeneration);
        var state = await _service.FetchStateAsync(projectId, cancellationToken).ConfigureAwait(false);
        await _dispatcher.InvokeAsync(() =>
        {
            if (IsCurrentMutation(projectId, session, mutation) &&
                stateRequest == _stateRequestGeneration)
            {
                ApplyState(state);
            }
        }, cancellationToken).ConfigureAwait(false);
    }

    private bool TryGetSession(
        out string projectId,
        out long session,
        out CancellationToken cancellationToken)
    {
        projectId = ProjectId ?? string.Empty;
        session = _sessionGeneration;
        cancellationToken = _sessionCancellation?.Token ?? CancellationToken.None;
        return projectId.Length > 0 && _sessionCancellation is not null && !cancellationToken.IsCancellationRequested;
    }

    private bool TryBeginMutation(
        out string projectId,
        out long session,
        out long mutation,
        out CancellationToken cancellationToken)
    {
        projectId = string.Empty;
        session = _sessionGeneration;
        mutation = 0;
        cancellationToken = CancellationToken.None;
        if (IsMutating || !TryGetSession(out projectId, out session, out cancellationToken))
        {
            return false;
        }

        Interlocked.Increment(ref _loadGeneration);
        mutation = Interlocked.Increment(ref _mutationGeneration);
        IsMutating = true;
        ErrorMessage = null;
        NoticeMessage = null;
        return true;
    }

    private bool IsCurrentSession(string projectId, long session) =>
        session == _sessionGeneration &&
        string.Equals(ProjectId, projectId, StringComparison.Ordinal) &&
        _sessionCancellation is { IsCancellationRequested: false };

    private bool IsCurrentLoad(string projectId, long session, long load) =>
        IsCurrentSession(projectId, session) && load == _loadGeneration;

    private bool IsCurrentMutation(string projectId, long session, long mutation) =>
        IsCurrentSession(projectId, session) && mutation == _mutationGeneration;

    private Task SetMutationErrorAsync(
        string projectId,
        long session,
        long mutation,
        Exception exception) => _dispatcher.InvokeAsync(() =>
    {
        if (IsCurrentMutation(projectId, session, mutation))
        {
            ErrorMessage = exception.Message;
        }
    });

    private Task SetMutationNoticeAsync(
        string projectId,
        long session,
        long mutation,
        string message) => _dispatcher.InvokeAsync(() =>
    {
        if (IsCurrentMutation(projectId, session, mutation))
        {
            NoticeMessage = message;
        }
    });

    private Task FinishMutationAsync(string projectId, long session, long mutation) =>
        _dispatcher.InvokeAsync(() =>
        {
            if (IsCurrentMutation(projectId, session, mutation))
            {
                IsMutating = false;
            }
        });

    private void ApplyCatalog(ProjectRunCatalog catalog, string? preferredTargetId)
    {
        CatalogErrorMessage = catalog.ErrorMessage;
        Targets.Clear();
        foreach (var target in catalog.Targets)
        {
            Targets.Add(target);
        }

        var selectedId = preferredTargetId;
        if (selectedId is null || Targets.All(value => value.Id != selectedId))
        {
            selectedId = catalog.DefaultTargetId
                ?? Targets.FirstOrDefault(static value => value.IsDefault)?.Id
                ?? Targets.FirstOrDefault()?.Id;
        }

        SelectedTarget = Targets.FirstOrDefault(value => value.Id == selectedId);
        if (!IsRunning && !IsBusy)
        {
            RunStatus = catalog.Status;
        }
    }

    private void ApplyState(ProjectRunState state)
    {
        RunStatus = state.Status;
        IsBusy = state.IsBusy;
        IsRunning = state.IsRunning;
        Instances.Clear();
        foreach (var instance in state.Instances)
        {
            Instances.Add(instance);
        }
    }

    private void ApplyEnvironment(ProjectRunEnvironment environment)
    {
        _customToolchains = new Dictionary<string, ProjectRunCustomToolchain>(
            environment.CustomToolchains,
            StringComparer.Ordinal);
        TerminalUiEnabled = environment.TerminalUiEnabled;

        ValidationIssues.Clear();
        foreach (var issue in environment.ValidationIssues)
        {
            ValidationIssues.Add(issue);
        }

        ConfigurationFiles.Clear();
        foreach (var file in environment.ConfigurationFiles)
        {
            ConfigurationFiles.Add(file);
        }

        Toolchains.Clear();
        var kinds = environment.ToolchainOptions.Keys
            .Concat(environment.SelectedToolchains.Keys)
            .Distinct(StringComparer.Ordinal)
            .OrderBy(static value => value, StringComparer.CurrentCultureIgnoreCase);
        foreach (var kind in kinds)
        {
            environment.ToolchainOptions.TryGetValue(kind, out var options);
            environment.SelectedToolchains.TryGetValue(kind, out var selectedId);
            Toolchains.Add(new ProjectRunToolchainSelectionViewModel(
                kind,
                options ?? Array.Empty<ProjectRunToolchainOption>(),
                selectedId));
        }

        EnvironmentVariables.Clear();
        foreach (var pair in environment.EnvironmentVariables.OrderBy(
                     static value => value.Key,
                     StringComparer.CurrentCultureIgnoreCase))
        {
            EnvironmentVariables.Add(new ProjectRunEnvironmentVariableViewModel(pair.Key, pair.Value));
        }
    }

    private void Reset(WorkspaceProject project)
    {
        ProjectId = project.Id;
        ProjectName = project.Name;
        ProjectRoot = project.DisplayRootPath ?? project.RootPath;
        RunStatus = "loading";
        IsRunning = false;
        IsBusy = false;
        IsLoading = true;
        IsMutating = false;
        SelectedTarget = null;
        TerminalUiEnabled = false;
        CatalogErrorMessage = null;
        ErrorMessage = null;
        NoticeMessage = null;
        _customToolchains = new Dictionary<string, ProjectRunCustomToolchain>();
        Targets.Clear();
        Instances.Clear();
        ValidationIssues.Clear();
        Toolchains.Clear();
        EnvironmentVariables.Clear();
        ConfigurationFiles.Clear();
    }

    private void ResetClosed()
    {
        ProjectId = null;
        ProjectName = string.Empty;
        ProjectRoot = null;
        RunStatus = "idle";
        IsRunning = false;
        IsBusy = false;
        IsLoading = false;
        IsMutating = false;
        SelectedTarget = null;
        TerminalUiEnabled = false;
        CatalogErrorMessage = null;
        ErrorMessage = null;
        NoticeMessage = null;
        _customToolchains = new Dictionary<string, ProjectRunCustomToolchain>();
        Targets.Clear();
        Instances.Clear();
        ValidationIssues.Clear();
        Toolchains.Clear();
        EnvironmentVariables.Clear();
        ConfigurationFiles.Clear();
    }

    private void CancelSession()
    {
        Interlocked.Increment(ref _loadGeneration);
        Interlocked.Increment(ref _mutationGeneration);
        Interlocked.Increment(ref _stateRequestGeneration);
        _sessionCancellation?.Cancel();
        _sessionCancellation?.Dispose();
        _sessionCancellation = null;
    }

    private string LocalizeStatus(string status) => status.ToLowerInvariant() switch
    {
        "running" => L("运行中", "Running"),
        "starting" => L("启动中", "Starting"),
        "stopping" => L("正在停止", "Stopping"),
        "ready" => L("就绪", "Ready"),
        "stopped" or "exited" => L("已停止", "Stopped"),
        "error" or "failed" => L("异常", "Error"),
        "idle" => L("空闲", "Idle"),
        "loading" => L("加载中", "Loading"),
        _ => status,
    };

    private string L(string chinese, string english) => _localization?.Text(chinese, english) ?? chinese;

    private void OnLocalizationChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e) =>
        OnPropertyChanged(nameof(RunStatusLabel));
}

public sealed partial class ProjectRunToolchainSelectionViewModel : ObservableObject
{
    public ProjectRunToolchainSelectionViewModel(
        string kind,
        IReadOnlyList<ProjectRunToolchainOption> options,
        string? selectedOptionId)
    {
        Kind = kind;
        Title = ToolchainTitle(kind);
        Options.Add(new ProjectRunToolchainChoice(
            string.Empty,
            options.FirstOrDefault()?.Label is { } automaticLabel
                ? $"自动 · {automaticLabel}"
                : "自动" ,
            options.FirstOrDefault()?.Path ?? string.Empty));
        foreach (var option in options)
        {
            Options.Add(new ProjectRunToolchainChoice(
                option.Id,
                string.IsNullOrWhiteSpace(option.Version)
                    ? option.Label
                    : $"{option.Label} · {option.Version}",
                option.Path));
        }

        if (!string.IsNullOrWhiteSpace(selectedOptionId) &&
            Options.All(value => value.Id != selectedOptionId))
        {
            Options.Add(new ProjectRunToolchainChoice(selectedOptionId, $"手动 · {selectedOptionId}", selectedOptionId));
        }

        _selectedOptionId = selectedOptionId ?? string.Empty;
    }

    public string Kind { get; }

    public string Title { get; }

    public ObservableCollection<ProjectRunToolchainChoice> Options { get; } = [];

    [ObservableProperty]
    private string? _selectedOptionId;

    public string SelectedPath => Options.FirstOrDefault(value => value.Id == SelectedOptionId)?.Path ?? string.Empty;

    partial void OnSelectedOptionIdChanged(string? value) => OnPropertyChanged(nameof(SelectedPath));

    private static string ToolchainTitle(string kind) => kind.ToLowerInvariant() switch
    {
        "java_home" => "JDK",
        "java" => "JDK / Java",
        "mvn" => "Maven",
        "gradle" => "Gradle",
        "python" => "Python",
        "node" => "Node.js",
        "npm" => "npm",
        "pnpm" => "pnpm",
        "yarn" => "Yarn",
        "cargo" => "Cargo",
        "go" => "Go",
        "swift" => "Swift",
        _ => kind,
    };
}

public sealed record ProjectRunToolchainChoice(string Id, string Label, string Path);

public sealed partial class ProjectRunEnvironmentVariableViewModel : ObservableObject
{
    public ProjectRunEnvironmentVariableViewModel(string key = "", string value = "")
    {
        _key = key;
        _value = value;
    }

    public Guid Id { get; } = Guid.NewGuid();

    [ObservableProperty]
    private string _key;

    [ObservableProperty]
    private string _value;
}
