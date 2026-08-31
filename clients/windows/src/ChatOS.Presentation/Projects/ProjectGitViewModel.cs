using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Projects;

public sealed partial class ProjectGitViewModel : ObservableObject, IDisposable
{
    private readonly IProjectGitService _service;
    private readonly IUiDispatcher _dispatcher;
    private readonly LocalizationViewModel? _localization;
    private CancellationTokenSource? _sessionCancellation;
    private CancellationTokenSource? _operationCancellation;
    private long _generation;

    public ProjectGitViewModel(
        IProjectGitService service,
        IUiDispatcher dispatcher,
        LocalizationViewModel? localization = null)
    {
        _service = service;
        _dispatcher = dispatcher;
        _localization = localization;
        if (_localization is not null) _localization.PropertyChanged += OnLocalizationChanged;
        Changes.CollectionChanged += (_, _) =>
        {
            OnPropertyChanged(nameof(HasChanges));
            OnPropertyChanged(nameof(HasStagedChanges));
            OnPropertyChanged(nameof(HasWorkingTreeChanges));
            OnPropertyChanged(nameof(CanCommit));
        };
        Branches.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasBranches));
        Commits.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasCommits));
        Remotes.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasRemotes));
    }

    public ObservableCollection<ProjectGitChange> Changes { get; } = [];

    public ObservableCollection<ProjectGitBranch> Branches { get; } = [];

    public ObservableCollection<ProjectGitCommit> Commits { get; } = [];

    public ObservableCollection<ProjectGitRemote> Remotes { get; } = [];

    public bool HasChanges => Changes.Count > 0;

    public bool HasStagedChanges => Changes.Any(static change => change.HasStagedChanges);

    public bool HasWorkingTreeChanges => Changes.Any(static change => change.HasWorkingTreeChanges);

    public bool HasBranches => Branches.Count > 0;

    public bool HasCommits => Commits.Count > 0;

    public bool HasRemotes => Remotes.Count > 0;

    public bool CanCommit => HasStagedChanges && CommitMessage.Trim().Length > 0 && !IsMutating;

    public bool CanCancel => IsLoading || IsMutating;

    public bool HasDiff => SelectedDiff is not null;

    public bool IsDiffEmpty => SelectedDiff is { Content.Length: 0 };

    public string HeadLabel => CurrentBranch ??
        (DetachedCommit is not null ? $"HEAD {DetachedCommit}" : L("尚无提交", "No commits yet"));

    public string SyncLabel
    {
        get
        {
            if (Upstream is null)
            {
                return L("尚未关联远程分支", "No upstream branch");
            }

            if (AheadCount == 0 && BehindCount == 0)
            {
                return L($"已与 {Upstream} 同步", $"In sync with {Upstream}");
            }

            return L(
                $"{Upstream} · 领先 {AheadCount} · 落后 {BehindCount}",
                $"{Upstream} · {AheadCount} ahead · {BehindCount} behind");
        }
    }

    [ObservableProperty]
    private string? _projectRoot;

    [ObservableProperty]
    private string _projectName = string.Empty;

    [ObservableProperty]
    private bool _isOpen;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanCancel))]
    private bool _isLoading;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanCommit))]
    [NotifyPropertyChangedFor(nameof(CanCancel))]
    private bool _isMutating;

    [ObservableProperty]
    private bool _isRepository;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HeadLabel))]
    private string? _currentBranch;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HeadLabel))]
    private string? _detachedCommit;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(SyncLabel))]
    private string? _upstream;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(SyncLabel))]
    private int _aheadCount;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(SyncLabel))]
    private int _behindCount;

    [ObservableProperty]
    private string? _repositoryRoot;

    [ObservableProperty]
    private ProjectGitChange? _selectedChange;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasDiff))]
    [NotifyPropertyChangedFor(nameof(IsDiffEmpty))]
    private ProjectGitDiff? _selectedDiff;

    [ObservableProperty]
    private ProjectGitBranch? _selectedBranch;

    [ObservableProperty]
    private ProjectGitRemote? _selectedRemote;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanCommit))]
    private string _commitMessage = string.Empty;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private string? _actionMessage;

    public async Task OpenAsync(
        WorkspaceProject project,
        CancellationToken cancellationToken = default)
    {
        CancelSession();
        _sessionCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        var token = _sessionCancellation.Token;
        var generation = Interlocked.Increment(ref _generation);
        await _dispatcher.InvokeAsync(() => Reset(project), token).ConfigureAwait(false);
        if (string.IsNullOrWhiteSpace(project.RootPath))
        {
            await _dispatcher.InvokeAsync(() =>
            {
                ErrorMessage = L("这个项目没有可访问的工作区路径。", "This project has no accessible workspace path.");
                IsLoading = false;
            }, token).ConfigureAwait(false);
            return;
        }

        await LoadSnapshotAsync(generation, token).ConfigureAwait(false);
    }

    public Task CloseAsync(CancellationToken cancellationToken = default)
    {
        CancelSession();
        return _dispatcher.InvokeAsync(() => Reset(null), cancellationToken);
    }

    [RelayCommand]
    private Task RefreshAsync()
    {
        if (_sessionCancellation is null || ProjectRoot is null || IsLoading || IsMutating)
        {
            return Task.CompletedTask;
        }

        return LoadSnapshotAsync(
            Interlocked.Increment(ref _generation),
            _sessionCancellation.Token);
    }

    [RelayCommand]
    private Task InitializeRepositoryAsync() => MutateAsync(
        L("Git 仓库已初始化。", "The Git repository was initialized."),
        (root, token) => _service.InitializeRepositoryAsync(root, token));

    [RelayCommand]
    private Task OpenDiffAsync(ProjectGitDiffRequest? request)
    {
        if (request is null || _sessionCancellation is null || ProjectRoot is null || IsLoading || IsMutating)
        {
            return Task.CompletedTask;
        }

        return LoadDiffAsync(request, _sessionCancellation.Token);
    }

    [RelayCommand]
    private Task StageChangeAsync(ProjectGitChange? change) => change is null
        ? Task.CompletedTask
        : MutateAsync(
            L($"已暂存 {change.Path}。", $"Staged {change.Path}."),
            (root, token) => _service.StageAsync(root, [change.Path], token));

    [RelayCommand]
    private Task UnstageChangeAsync(ProjectGitChange? change) => change is null
        ? Task.CompletedTask
        : MutateAsync(
            L($"已取消暂存 {change.Path}。", $"Unstaged {change.Path}."),
            (root, token) => _service.UnstageAsync(root, [change.Path], token));

    [RelayCommand]
    private Task StageAllAsync()
    {
        var paths = Changes
            .Where(static change => change.HasWorkingTreeChanges)
            .Select(static change => change.Path)
            .Distinct(StringComparer.Ordinal)
            .ToArray();
        return MutateAsync(
            L("全部工作区修改已暂存。", "All working-tree changes were staged."),
            (root, token) => _service.StageAsync(root, paths, token));
    }

    [RelayCommand]
    private Task UnstageAllAsync()
    {
        var paths = Changes
            .Where(static change => change.HasStagedChanges)
            .Select(static change => change.Path)
            .Distinct(StringComparer.Ordinal)
            .ToArray();
        return MutateAsync(
            L("全部暂存修改已移回工作区。", "All staged changes were moved back to the working tree."),
            (root, token) => _service.UnstageAsync(root, paths, token));
    }

    [RelayCommand]
    private Task CommitAsync()
    {
        if (!CanCommit)
        {
            ErrorMessage = HasStagedChanges
                ? L("请输入提交说明。", "Enter a commit message.")
                : L("请先暂存需要提交的修改。", "Stage the changes to commit first.");
            return Task.CompletedTask;
        }

        var message = CommitMessage.Trim();
        return MutateAsync(
            L("提交已创建。", "The commit was created."),
            (root, token) => _service.CommitAsync(root, message, token),
            () => CommitMessage = string.Empty);
    }

    [RelayCommand]
    private Task SwitchBranchAsync(ProjectGitBranch? branch) => branch is null || branch.IsCurrent
        ? Task.CompletedTask
        : MutateAsync(
            L($"已切换到 {branch.Name}。", $"Switched to {branch.Name}."),
            (root, token) => _service.SwitchBranchAsync(root, branch.Name, token));

    [RelayCommand]
    private Task CreateBranchAsync(ProjectGitBranchDraft? draft) => draft is null
        ? Task.CompletedTask
        : MutateAsync(
            draft.SwitchToBranch
                ? L($"已创建并切换到 {draft.Name}。", $"Created and switched to {draft.Name}.")
                : L($"已创建分支 {draft.Name}。", $"Created branch {draft.Name}."),
            (root, token) => _service.CreateBranchAsync(root, draft.Name, draft.SwitchToBranch, token));

    [RelayCommand]
    private Task MergeBranchAsync(ProjectGitBranch? branch) => branch is null || branch.IsCurrent
        ? Task.CompletedTask
        : MutateAsync(
            L($"已合并 {branch.Name}。", $"Merged {branch.Name}."),
            (root, token) => _service.MergeBranchAsync(root, branch.Name, token));

    [RelayCommand]
    private Task SaveRemoteAsync(ProjectGitRemoteDraft? draft) => draft is null
        ? Task.CompletedTask
        : MutateAsync(
            L("远程仓库配置已保存。", "The remote configuration was saved."),
            (root, token) => _service.SaveRemoteAsync(
                root,
                draft.OriginalName,
                draft.Name,
                draft.Url,
                token));

    [RelayCommand]
    private Task RemoveRemoteAsync(ProjectGitRemote? remote) => remote is null
        ? Task.CompletedTask
        : MutateAsync(
            L($"远程仓库 {remote.Name} 已移除。", $"Remote {remote.Name} was removed."),
            (root, token) => _service.RemoveRemoteAsync(root, remote.Name, token));

    [RelayCommand]
    private Task PullAsync() => MutateAsync(
        L("已从远程拉取最新提交。", "Pulled the latest commits from the remote."),
        (root, token) => _service.PullAsync(root, token));

    [RelayCommand]
    private Task PushAsync() => MutateAsync(
        L("当前分支已发布到远程。", "The current branch was published to the remote."),
        (root, token) => _service.PushAsync(root, token));

    [RelayCommand]
    private void CancelOperation()
    {
        if (_operationCancellation is null || _operationCancellation.IsCancellationRequested)
        {
            return;
        }

        ActionMessage = L("正在取消 Git 操作…", "Cancelling the Git operation…");
        _operationCancellation.Cancel();
    }

    public void Dispose()
    {
        CancelSession();
        if (_localization is not null) _localization.PropertyChanged -= OnLocalizationChanged;
    }

    private async Task LoadSnapshotAsync(long generation, CancellationToken sessionToken)
    {
        if (ProjectRoot is not { } root)
        {
            return;
        }

        var token = BeginOperation(sessionToken);
        await _dispatcher.InvokeAsync(() =>
        {
            IsLoading = true;
            ErrorMessage = null;
            ActionMessage = null;
        }, sessionToken).ConfigureAwait(false);
        try
        {
            var snapshot = await _service.SnapshotAsync(root, token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _generation)
                {
                    ApplySnapshot(snapshot);
                }
            }, sessionToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
            await ShowCancellationAsync(sessionToken).ConfigureAwait(false);
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message, sessionToken)
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() => IsLoading = false, sessionToken).ConfigureAwait(false);
        }
    }

    private async Task LoadDiffAsync(ProjectGitDiffRequest request, CancellationToken sessionToken)
    {
        if (ProjectRoot is not { } root)
        {
            return;
        }

        var generation = Interlocked.Increment(ref _generation);
        var token = BeginOperation(sessionToken);
        await _dispatcher.InvokeAsync(() =>
        {
            IsLoading = true;
            SelectedChange = request.Change;
            ErrorMessage = null;
            ActionMessage = null;
        }, sessionToken).ConfigureAwait(false);
        try
        {
            var diff = await _service.DiffAsync(root, request.Change, request.Staged, token)
                .ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _generation)
                {
                    SelectedDiff = diff;
                }
            }, sessionToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
            await ShowCancellationAsync(sessionToken).ConfigureAwait(false);
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message, sessionToken)
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() => IsLoading = false, sessionToken).ConfigureAwait(false);
        }
    }

    private async Task MutateAsync(
        string successMessage,
        Func<string, CancellationToken, Task> mutation,
        Action? afterSuccess = null)
    {
        if (_sessionCancellation is null || ProjectRoot is not { } root || IsLoading || IsMutating)
        {
            return;
        }

        var sessionToken = _sessionCancellation.Token;
        var generation = Interlocked.Increment(ref _generation);
        var token = BeginOperation(sessionToken);
        await _dispatcher.InvokeAsync(() =>
        {
            IsMutating = true;
            ErrorMessage = null;
            ActionMessage = null;
        }, sessionToken).ConfigureAwait(false);
        try
        {
            await mutation(root, token).ConfigureAwait(false);
            var snapshot = await _service.SnapshotAsync(root, token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation != _generation)
                {
                    return;
                }

                ApplySnapshot(snapshot);
                afterSuccess?.Invoke();
                ActionMessage = successMessage;
            }, sessionToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
            await ShowCancellationAsync(sessionToken).ConfigureAwait(false);
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message, sessionToken)
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() => IsMutating = false, sessionToken).ConfigureAwait(false);
        }
    }

    private CancellationToken BeginOperation(CancellationToken sessionToken)
    {
        _operationCancellation?.Cancel();
        _operationCancellation?.Dispose();
        _operationCancellation = CancellationTokenSource.CreateLinkedTokenSource(sessionToken);
        return _operationCancellation.Token;
    }

    private Task ShowCancellationAsync(CancellationToken sessionToken)
    {
        if (sessionToken.IsCancellationRequested)
        {
            return Task.CompletedTask;
        }

        return _dispatcher.InvokeAsync(() =>
        {
            ErrorMessage = null;
            ActionMessage = L("Git 操作已取消。", "The Git operation was cancelled.");
        });
    }

    private void ApplySnapshot(ProjectGitSnapshot snapshot)
    {
        IsRepository = snapshot.IsRepository;
        RepositoryRoot = snapshot.RepositoryRoot;
        CurrentBranch = snapshot.CurrentBranch;
        DetachedCommit = snapshot.DetachedCommit;
        Upstream = snapshot.Upstream;
        AheadCount = snapshot.AheadCount;
        BehindCount = snapshot.BehindCount;
        Replace(Changes, snapshot.Changes);
        Replace(Branches, snapshot.Branches);
        Replace(Commits, snapshot.Commits);
        Replace(Remotes, snapshot.Remotes);
        SelectedBranch = Branches.FirstOrDefault(static branch => branch.IsCurrent);
        SelectedRemote = Remotes.FirstOrDefault();
        if (SelectedChange is not null)
        {
            SelectedChange = Changes.FirstOrDefault(change => change.Path == SelectedChange.Path);
            if (SelectedChange is null)
            {
                SelectedDiff = null;
            }
        }
    }

    private void Reset(WorkspaceProject? project)
    {
        ProjectRoot = project?.RootPath;
        ProjectName = project?.Name ?? string.Empty;
        IsOpen = project is not null;
        IsLoading = project is not null;
        IsMutating = false;
        IsRepository = false;
        RepositoryRoot = null;
        CurrentBranch = null;
        DetachedCommit = null;
        Upstream = null;
        AheadCount = 0;
        BehindCount = 0;
        SelectedChange = null;
        SelectedDiff = null;
        SelectedBranch = null;
        SelectedRemote = null;
        CommitMessage = string.Empty;
        ErrorMessage = null;
        ActionMessage = null;
        Changes.Clear();
        Branches.Clear();
        Commits.Clear();
        Remotes.Clear();
    }

    private void CancelSession()
    {
        _operationCancellation?.Cancel();
        _operationCancellation?.Dispose();
        _operationCancellation = null;
        _sessionCancellation?.Cancel();
        _sessionCancellation?.Dispose();
        _sessionCancellation = null;
        Interlocked.Increment(ref _generation);
    }

    private static void Replace<T>(ObservableCollection<T> target, IEnumerable<T> values)
    {
        target.Clear();
        foreach (var value in values)
        {
            target.Add(value);
        }
    }

    private string L(string chinese, string english) => _localization?.Text(chinese, english) ?? chinese;

    private void OnLocalizationChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
    {
        OnPropertyChanged(nameof(HeadLabel));
        OnPropertyChanged(nameof(SyncLabel));
    }
}
