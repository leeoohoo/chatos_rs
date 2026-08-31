using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Projects;

public sealed partial class ProjectPlanViewModel : ObservableObject, IDisposable
{
    private readonly IProjectPlanService _service;
    private readonly IProjectExecutionService _executionService;
    private readonly IUiDispatcher _dispatcher;
    private readonly LocalizationViewModel? _localization;
    private CancellationTokenSource? _sessionCancellation;
    private long _generation;
    private long _executionMutationGeneration;

    public ProjectPlanViewModel(
        IProjectPlanService service,
        IProjectExecutionService executionService,
        IUiDispatcher dispatcher,
        LocalizationViewModel? localization = null)
    {
        _service = service;
        _executionService = executionService;
        _dispatcher = dispatcher;
        _localization = localization;
        if (_localization is not null) _localization.PropertyChanged += OnLocalizationChanged;
        Requirements.CollectionChanged += (_, _) => OnPropertyChanged(nameof(IsEmpty));
        WorkItems.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasWorkItems));
        Documents.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasDocuments));
    }

    public ObservableCollection<ProjectRequirementItemViewModel> Requirements { get; } = [];

    public ObservableCollection<ProjectWorkItem> WorkItems { get; } = [];

    public ObservableCollection<ProjectRequirementDocument> Documents { get; } = [];

    public bool IsEmpty => Requirements.Count == 0;

    public bool HasWorkItems => WorkItems.Count > 0;

    public bool HasDocuments => Documents.Count > 0;

    public bool HasSelection => SelectedRequirement is not null;

    public bool HasExecution => Execution is not null;

    public bool HasExecutionIdentity => TryCreateExecutionIdentity(out _);

    public bool CanConfirmExecution =>
        HasExecutionIdentity &&
        !IsCreatingExecution &&
        !IsMutatingExecution &&
        Execution is { HasStartedRuns: false } &&
        NormalizeExecutionStatus(Execution) is "awaiting_confirmation" or "pending_confirmation" or "review_required" or "pending";

    public bool CanStopExecution =>
        HasExecutionIdentity &&
        !IsCreatingExecution &&
        !IsMutatingExecution &&
        NormalizeExecutionStatus(Execution) is not ("completed" or "succeeded" or "success" or "stopped" or "cancelled" or "canceled");

    public string ExecutionStatusLabel => LocalizeExecutionStatus(NormalizeExecutionStatus(Execution));

    public string ExecutionStopLabel => NormalizeExecutionStatus(Execution) switch
    {
        "awaiting_confirmation" or "pending_confirmation" or "review_required" or "pending" => L("放弃计划", "Discard plan"),
        "failed" or "blocked" or "error" => L("清理计划", "Clear plan"),
        _ => L("停止执行", "Stop execution"),
    };

    [ObservableProperty]
    private string? _projectId;

    [ObservableProperty]
    private string _projectName = string.Empty;

    [ObservableProperty]
    private bool _isLoading;

    [ObservableProperty]
    private bool _isLoadingDetail;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanConfirmExecution))]
    [NotifyPropertyChangedFor(nameof(CanStopExecution))]
    private bool _isCreatingExecution;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanConfirmExecution))]
    [NotifyPropertyChangedFor(nameof(CanStopExecution))]
    private bool _isMutatingExecution;

    [ObservableProperty]
    private ProjectPlanCounts _counts = new(0, 0, 0, 0);

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasSelection))]
    private ProjectRequirementItemViewModel? _selectedRequirement;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasExecution))]
    [NotifyPropertyChangedFor(nameof(HasExecutionIdentity))]
    [NotifyPropertyChangedFor(nameof(CanConfirmExecution))]
    [NotifyPropertyChangedFor(nameof(CanStopExecution))]
    [NotifyPropertyChangedFor(nameof(ExecutionStatusLabel))]
    [NotifyPropertyChangedFor(nameof(ExecutionStopLabel))]
    private ProjectRequirementExecutionLaunch? _execution;

    [ObservableProperty]
    private bool _includePrerequisiteDependents;

    [ObservableProperty]
    private string _planningFeedback = string.Empty;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private string? _executionActionMessage;

    public async Task OpenAsync(
        WorkspaceProject project,
        CancellationToken cancellationToken = default)
    {
        CancelSession();
        _sessionCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        var token = _sessionCancellation.Token;
        var generation = Interlocked.Increment(ref _generation);
        await _dispatcher.InvokeAsync(() => Reset(project), token);
        await LoadPlanInternalAsync(project.Id, generation, token).ConfigureAwait(false);
    }

    [RelayCommand]
    private Task RefreshAsync()
    {
        if (ProjectId is not { } projectId || _sessionCancellation is null)
        {
            return Task.CompletedTask;
        }

        return LoadPlanInternalAsync(
            projectId,
            Interlocked.Increment(ref _generation),
            _sessionCancellation.Token);
    }

    [RelayCommand]
    private async Task SelectRequirementAsync(ProjectRequirementItemViewModel? requirement)
    {
        if (requirement is null || ProjectId is not { } projectId || _sessionCancellation is null)
        {
            return;
        }

        SelectedRequirement = requirement;
        var generation = Interlocked.Increment(ref _generation);
        await LoadRequirementDetailInternalAsync(
            projectId,
            requirement.Id,
            generation,
            _sessionCancellation.Token).ConfigureAwait(false);
    }

    [RelayCommand]
    private async Task CreateExecutionAsync()
    {
        if (ProjectId is not { } projectId ||
            SelectedRequirement is not { } requirement ||
            _sessionCancellation is null ||
            IsCreatingExecution ||
            IsMutatingExecution)
        {
            return;
        }

        var token = _sessionCancellation.Token;
        var generation = Interlocked.Increment(ref _generation);
        var mutation = Interlocked.Increment(ref _executionMutationGeneration);
        IsCreatingExecution = true;
        ErrorMessage = null;
        ExecutionActionMessage = null;
        try
        {
            var execution = await _service.CreateExecutionAsync(
                projectId,
                requirement.Id,
                IncludePrerequisiteDependents,
                PlanningFeedback,
                token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (!IsCurrentExecutionMutation(projectId, requirement.Id, generation, mutation, token))
                {
                    return;
                }

                Execution = execution;
                PlanningFeedback = string.Empty;
                ExecutionActionMessage = L(
                    "执行计划已生成，请检查任务图后确认执行。",
                    "The execution plan is ready. Review the task graph, then confirm execution.");
            }).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (IsCurrentExecutionMutation(projectId, requirement.Id, generation, mutation, token))
                {
                    ErrorMessage = exception.Message;
                }
            })
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (IsCurrentExecutionMutation(projectId, requirement.Id, generation, mutation, token))
                {
                    IsCreatingExecution = false;
                }
            })
                .ConfigureAwait(false);
        }
    }

    [RelayCommand]
    private Task ConfirmExecutionAsync() => MutateExecutionAsync(confirm: true);

    [RelayCommand]
    private Task StopExecutionAsync() => MutateExecutionAsync(confirm: false);

    public void Dispose()
    {
        CancelSession();
        if (_localization is not null) _localization.PropertyChanged -= OnLocalizationChanged;
    }

    private async Task MutateExecutionAsync(bool confirm)
    {
        if (_sessionCancellation is null ||
            SelectedRequirement is not { } requirement ||
            !TryCreateExecutionIdentity(out var identity) ||
            IsCreatingExecution ||
            IsMutatingExecution ||
            (confirm ? !CanConfirmExecution : !CanStopExecution))
        {
            return;
        }

        var projectId = identity.ProjectId;
        var requirementId = identity.RequirementId;
        var token = _sessionCancellation.Token;
        var generation = Interlocked.Increment(ref _generation);
        var mutation = Interlocked.Increment(ref _executionMutationGeneration);
        IsMutatingExecution = true;
        ErrorMessage = null;
        ExecutionActionMessage = null;
        try
        {
            var action = confirm
                ? await _executionService.ConfirmExecutionAsync(identity, token).ConfigureAwait(false)
                : await _executionService.StopExecutionAsync(identity, token).ConfigureAwait(false);
            await Task.Delay(TimeSpan.FromMilliseconds(confirm ? 300 : 200), token).ConfigureAwait(false);
            var refreshed = await _executionService.FetchExecutionAsync(identity, token)
                .ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (!IsCurrentExecutionMutation(projectId, requirementId, generation, mutation, token))
                {
                    return;
                }

                Execution = refreshed ?? (confirm
                    ? Execution is { } current
                        ? current with
                        {
                            ConfirmationStatus = "confirmed",
                            HasStartedRuns = true,
                            OverallStatus = action.Status ?? "processing",
                        }
                        : null
                    : null);
                ExecutionActionMessage = confirm
                    ? L("已确认执行，任务将按依赖顺序运行。", "Execution confirmed. Tasks will run in dependency order.")
                    : L("执行计划已停止，未继续运行的任务已清理。", "The execution plan was stopped and tasks that had not continued were cleared.");
            }, token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (IsCurrentExecutionMutation(projectId, requirementId, generation, mutation, token))
                {
                    ErrorMessage = exception.Message;
                }
            }).ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (IsCurrentExecutionMutation(projectId, requirementId, generation, mutation, token))
                {
                    IsMutatingExecution = false;
                }
            }).ConfigureAwait(false);
        }
    }

    private async Task LoadPlanInternalAsync(
        string projectId,
        long generation,
        CancellationToken cancellationToken)
    {
        await _dispatcher.InvokeAsync(() =>
        {
            IsLoading = true;
            ErrorMessage = null;
        }, cancellationToken).ConfigureAwait(false);
        try
        {
            var snapshot = await _service.FetchPlanAsync(projectId, cancellationToken)
                .ConfigureAwait(false);
            var flattened = FlattenRequirements(snapshot.Requirements);
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation != _generation)
                {
                    return;
                }

                Requirements.Clear();
                foreach (var requirement in flattened)
                {
                    Requirements.Add(requirement);
                }

                Counts = snapshot.Counts;
                var selectedId = SelectedRequirement?.Id;
                SelectedRequirement = selectedId is null
                    ? Requirements.FirstOrDefault()
                    : Requirements.FirstOrDefault(value => value.Id == selectedId)
                        ?? Requirements.FirstOrDefault();
            }, cancellationToken).ConfigureAwait(false);

            if (generation == _generation && SelectedRequirement is { } selected)
            {
                await LoadRequirementDetailInternalAsync(
                    projectId,
                    selected.Id,
                    generation,
                    cancellationToken).ConfigureAwait(false);
            }
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _generation)
                {
                    IsLoading = false;
                }
            }).ConfigureAwait(false);
        }
    }

    private async Task LoadRequirementDetailInternalAsync(
        string projectId,
        string requirementId,
        long generation,
        CancellationToken cancellationToken)
    {
        await _dispatcher.InvokeAsync(() =>
        {
            IsLoadingDetail = true;
            ErrorMessage = null;
        }, cancellationToken).ConfigureAwait(false);
        try
        {
            var workItemsTask = _service.FetchWorkItemsAsync(projectId, requirementId, cancellationToken);
            var documentsTask = _service.FetchDocumentsAsync(projectId, requirementId, cancellationToken);
            var executionTask = _service.FetchExecutionAsync(projectId, requirementId, cancellationToken);
            await Task.WhenAll(workItemsTask, documentsTask, executionTask).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation != _generation || SelectedRequirement?.Id != requirementId)
                {
                    return;
                }

                WorkItems.Clear();
                foreach (var item in workItemsTask.Result.WorkItems)
                {
                    WorkItems.Add(item);
                }

                Documents.Clear();
                foreach (var document in documentsTask.Result)
                {
                    Documents.Add(document);
                }

                Execution = executionTask.Result;
                IncludePrerequisiteDependents = executionTask.Result?.IncludePrerequisiteDependents ?? false;
            }, cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _generation)
                {
                    IsLoadingDetail = false;
                }
            }).ConfigureAwait(false);
        }
    }

    private static IReadOnlyList<ProjectRequirementItemViewModel> FlattenRequirements(
        IReadOnlyList<ProjectRequirement> requirements)
    {
        var children = requirements
            .GroupBy(static value => value.ParentRequirementId ?? string.Empty)
            .ToDictionary(static group => group.Key, static group => group
                .OrderByDescending(static value => value.Priority)
                .ThenBy(static value => value.Title, StringComparer.CurrentCultureIgnoreCase)
                .ToArray());
        var result = new List<ProjectRequirementItemViewModel>();
        var visited = new HashSet<string>(StringComparer.Ordinal);

        void Append(ProjectRequirement requirement, int depth)
        {
            if (!visited.Add(requirement.Id))
            {
                return;
            }

            result.Add(new ProjectRequirementItemViewModel(requirement, depth));
            if (children.TryGetValue(requirement.Id, out var nested))
            {
                foreach (var child in nested)
                {
                    Append(child, depth + 1);
                }
            }
        }

        foreach (var root in requirements.Where(value =>
                     string.IsNullOrWhiteSpace(value.ParentRequirementId) ||
                     requirements.All(candidate => candidate.Id != value.ParentRequirementId)))
        {
            Append(root, 0);
        }

        foreach (var requirement in requirements)
        {
            Append(requirement, 0);
        }

        return result;
    }

    private bool TryCreateExecutionIdentity(out ProjectExecutionIdentity identity)
    {
        identity = null!;
        if (Execution is not { } execution)
        {
            return false;
        }

        var projectId = execution.ProjectId.Trim();
        var requirementId = execution.RequirementId.Trim();
        var executionGroupId = execution.ExecutionGroupId.Trim();
        var conversationId = execution.ConversationId.Trim();
        if (projectId.Length == 0 || requirementId.Length == 0 ||
            executionGroupId.Length == 0 || conversationId.Length == 0)
        {
            return false;
        }

        identity = new ProjectExecutionIdentity(
            projectId,
            requirementId,
            executionGroupId,
            conversationId,
            string.IsNullOrWhiteSpace(execution.ContactId) ? null : execution.ContactId.Trim());
        return true;
    }

    private bool IsCurrentExecutionMutation(
        string projectId,
        string requirementId,
        long generation,
        long mutation,
        CancellationToken token) =>
        _sessionCancellation?.Token == token &&
        !token.IsCancellationRequested &&
        generation == _generation &&
        mutation == _executionMutationGeneration &&
        string.Equals(ProjectId, projectId, StringComparison.Ordinal) &&
        string.Equals(SelectedRequirement?.Id, requirementId, StringComparison.Ordinal);

    private static string NormalizeExecutionStatus(ProjectRequirementExecutionLaunch? execution) =>
        (execution?.OverallStatus ?? execution?.ConfirmationStatus ?? string.Empty)
        .Trim()
        .ToLowerInvariant();

    private string LocalizeExecutionStatus(string status) => status switch
    {
        "planning" or "queued" => L("正在生成任务图", "Creating task graph"),
        "awaiting_confirmation" or "pending_confirmation" or "review_required" or "pending" => L("等待确认", "Awaiting confirmation"),
        "confirmed" or "processing" or "running" or "executing" or "in_progress" => L("执行中", "Running"),
        "completed" or "succeeded" or "success" => L("已完成", "Completed"),
        "blocked" => L("已阻塞", "Blocked"),
        "failed" or "error" => L("执行失败", "Failed"),
        "stopped" or "cancelled" or "canceled" => L("已停止", "Stopped"),
        _ when status.Length == 0 => L("未知", "Unknown"),
        _ => status,
    };

    private string L(string chinese, string english) => _localization?.Text(chinese, english) ?? chinese;

    private void OnLocalizationChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
    {
        OnPropertyChanged(nameof(ExecutionStatusLabel));
        OnPropertyChanged(nameof(ExecutionStopLabel));
    }

    private void Reset(WorkspaceProject project)
    {
        ProjectId = project.Id;
        ProjectName = project.Name;
        IsLoading = true;
        IsLoadingDetail = false;
        IsCreatingExecution = false;
        IsMutatingExecution = false;
        Counts = new ProjectPlanCounts(0, 0, 0, 0);
        SelectedRequirement = null;
        Execution = null;
        IncludePrerequisiteDependents = false;
        PlanningFeedback = string.Empty;
        ErrorMessage = null;
        ExecutionActionMessage = null;
        Requirements.Clear();
        WorkItems.Clear();
        Documents.Clear();
    }

    private void CancelSession()
    {
        Interlocked.Increment(ref _executionMutationGeneration);
        _sessionCancellation?.Cancel();
        _sessionCancellation?.Dispose();
        _sessionCancellation = null;
    }
}

public sealed class ProjectRequirementItemViewModel
{
    public ProjectRequirementItemViewModel(ProjectRequirement requirement, int depth)
    {
        Requirement = requirement;
        Depth = depth;
    }

    public ProjectRequirement Requirement { get; }

    public int Depth { get; }

    public string Id => Requirement.Id;

    public string Title => Requirement.Title;

    public string DisplayTitle => $"{new string('　', Depth)}{Requirement.Title}";

    public string Status => Requirement.Status;

    public string? Summary => Requirement.Summary;

    public string? Detail => Requirement.Detail;

    public string? AcceptanceCriteria => Requirement.AcceptanceCriteria;
}
