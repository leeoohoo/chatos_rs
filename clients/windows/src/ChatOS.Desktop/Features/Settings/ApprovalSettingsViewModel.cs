using System.Collections.ObjectModel;
using ChatOS.Connector.Approval;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Desktop.Features.Settings;

public sealed partial class ApprovalSettingsViewModel : ObservableObject, IDisposable
{
    private readonly CommandApprovalCoordinator _coordinator;
    private readonly IApprovalReviewerReadinessService? _reviewerReadiness;
    private readonly LocalizationViewModel _localization;
    private readonly IUiDispatcher _dispatcher;
    private readonly SettingsOperationRunner _operations;
    private readonly object _lifecycleGate = new();
    private readonly CancellationTokenSource _lifetime = new();
    private volatile bool _disposed;
    private bool _cancellationFinished;
    private int _activeOperations;
    private long _historyChangeVersion;
    private bool _historyRefreshPending;
    private IReadOnlyList<ConnectorApprovalHistoryEntry> _history = [];
    private int _historyPageIndex;
    private int _historyPageSize = 10;

    public ApprovalSettingsViewModel(
        CommandApprovalCoordinator coordinator,
        LocalizationViewModel localization,
        IUiDispatcher dispatcher,
        IApprovalReviewerReadinessService? reviewerReadiness = null)
    {
        _coordinator = coordinator;
        _localization = localization;
        _dispatcher = dispatcher;
        _operations = new SettingsOperationRunner(dispatcher);
        _reviewerReadiness = reviewerReadiness;
        _coordinator.PendingChanged += OnPendingChanged;
        _coordinator.DecisionRecorded += OnDecisionRecorded;
        _localization.PropertyChanged += OnLocalizationChanged;
    }

    public ObservableCollection<ApprovalPendingItemViewModel> Pending { get; } = [];

    public ObservableCollection<ApprovalHistoryItemViewModel> History { get; } = [];

    public bool HasPending => Pending.Count > 0;

    public bool HasHistory => _history.Count > 0;

    public IReadOnlyList<int> HistoryPageSizeOptions { get; } = Array.AsReadOnly(new[] { 10, 20, 50 });

    public int HistoryCount => _history.Count;
    public int HistoryPageIndex => _historyPageIndex;
    public int HistoryPageCount => Math.Max(1, (HistoryCount + HistoryPageSize - 1) / HistoryPageSize);
    public bool HasHistoryPagination => HistoryCount > HistoryPageSizeOptions[0];
    public bool CanRefreshHistory => !IsBusy && !IsHistoryRefreshing;

    public int HistoryPageSize
    {
        get => _historyPageSize;
        set
        {
            if (!HistoryPageSizeOptions.Contains(value) || !SetProperty(ref _historyPageSize, value)) return;
            _historyPageIndex = 0;
            ApplyHistoryPage();
        }
    }

    public string HistoryRangeLabel => HistoryCount == 0
        ? _localization.Text("0 条", "0 entries")
        : $"{HistoryPageIndex * HistoryPageSize + 1}–{Math.Min(HistoryCount, (HistoryPageIndex + 1) * HistoryPageSize)} / {HistoryCount}";

    public string HistoryPageLabel => _localization.Text(
        $"第 {HistoryPageIndex + 1} 页，共 {HistoryPageCount} 页",
        $"Page {HistoryPageIndex + 1} of {HistoryPageCount}");

    public string HistoryScopeLabel => _localization.Text("本机最近 1,000 条审批记录", "Up to 1,000 recent approvals on this device");
    public string HistoryPageSizeLabel => _localization.Text("每页条数", "Entries per page");
    public string PreviousHistoryPageLabel => _localization.Text("上一页", "Previous page");
    public string NextHistoryPageLabel => _localization.Text("下一页", "Next page");

    private bool CanPreviousHistoryPage() => HistoryPageIndex > 0;
    private bool CanNextHistoryPage() => HistoryPageIndex + 1 < HistoryPageCount;

    [RelayCommand(CanExecute = nameof(CanPreviousHistoryPage))]
    private void PreviousHistoryPage()
    {
        if (!CanPreviousHistoryPage()) return;
        _historyPageIndex--;
        ApplyHistoryPage();
    }

    [RelayCommand(CanExecute = nameof(CanNextHistoryPage))]
    private void NextHistoryPage()
    {
        if (!CanNextHistoryPage()) return;
        _historyPageIndex++;
        ApplyHistoryPage();
    }

    public string NoPendingLabel => _localization.Text(
        "当前没有等待审批的操作",
        "There are no operations awaiting approval");

    public string NoPendingDescription => _localization.Text(
        "任务会停在这里，直到用户或审批模型作出决定。",
        "Tasks wait here until the user or approval model makes a decision.");

    public string NoHistoryLabel => _localization.Text(
        "还没有审批记录",
        "There is no approval history yet");

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(ModeDescription))]
    [NotifyPropertyChangedFor(nameof(AutomaticReviewerStatus))]
    [NotifyPropertyChangedFor(nameof(CanRetryReviewerReadiness))]
    private ConnectorApprovalMode _mode = ConnectorApprovalMode.RequestApproval;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanRefreshHistory))]
    [NotifyPropertyChangedFor(nameof(CanRetryReviewerReadiness))]
    private bool _isBusy;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanRefreshHistory))]
    [NotifyPropertyChangedFor(nameof(CanRetryReviewerReadiness))]
    private bool _isHistoryRefreshing;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(AutomaticReviewerStatus))]
    [NotifyPropertyChangedFor(nameof(CanRetryReviewerReadiness))]
    private bool _isReviewerReadinessChecking;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasReviewerReadinessError))]
    [NotifyPropertyChangedFor(nameof(AutomaticReviewerStatus))]
    private string? _reviewerReadinessErrorMessage;

    public bool HasReviewerReadinessError => !string.IsNullOrEmpty(ReviewerReadinessErrorMessage);
    public bool CanRetryReviewerReadiness => Mode == ConnectorApprovalMode.AutoApproval &&
        !IsBusy && !IsHistoryRefreshing && !IsReviewerReadinessChecking;
    public string ReviewerReadinessErrorDescription => _localization.Text(
        "审批模型状态检查失败，当前审批模式仍然有效。可重试检查。",
        "Could not check the approval model. The current approval mode remains in effect. Retry the check.");
    public string RetryReviewerReadinessLabel => _localization.Text("重新检查审批模型", "Check approval model again");

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private string? _actionMessage;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasHistorySyncError))]
    private string? _historySyncErrorMessage;

    public bool HasHistorySyncError => !string.IsNullOrEmpty(HistorySyncErrorMessage);
    public string HistorySyncErrorDescription => _localization.Text(
        "审批记录未能刷新，已保留上次记录。请点击刷新重试。",
        "Approval history could not be refreshed. Previous entries are preserved. Select Refresh to retry.");

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(AutomaticReviewerStatus))]
    private ApprovalReviewerReadinessState _reviewerReadinessState =
        ApprovalReviewerReadinessState.ManagedConfigurationInvalid;

    public string SectionDescription => _localization.Text(
        "设置本机命令的默认审批策略，并查看等待处理与最近审计记录。",
        "Set the default policy for local commands and review pending decisions and recent audit history.");

    public string ModeDescription => Mode switch
    {
        ConnectorApprovalMode.AutoApproval => _localization.Text(
            "由本机审批模型判断；模型不可用或无法判断时仍会请求用户确认。",
            "The local approval model decides; unavailable or uncertain reviews still ask the user."),
        ConnectorApprovalMode.FullControl => _localization.Text(
            "任务可直接执行高风险命令，只应在完全受信任的设备和项目中使用。",
            "Tasks may execute high-risk commands directly. Use only on fully trusted devices and projects."),
        _ => _localization.Text(
            "敏感命令逐条确认，这是推荐的默认策略。",
            "Confirm sensitive commands individually. This is the recommended default."),
    };

    public string AutomaticReviewerStatus => Mode == ConnectorApprovalMode.AutoApproval
        ? IsReviewerReadinessChecking
            ? _localization.Text("正在检查本机审批模型状态…", "Checking the local approval model…")
            : HasReviewerReadinessError
                ? _localization.Text(
                    "无法确认本机审批模型状态；模型未就绪或无法判断时仍会请求用户确认。",
                    "The local approval model status is unknown; unavailable or uncertain reviews still ask the user.")
                : ReviewerReadinessState switch
        {
            ApprovalReviewerReadinessState.Ready => _localization.Text(
                "Windows AI 审批已就绪；无法判断时仍会安全回退为用户确认。",
                "Windows AI approval is ready; uncertain reviews still safely fall back to user confirmation."),
            ApprovalReviewerReadinessState.ModelNotSelected => _localization.Text(
                "尚未选择本机审批模型，自动审批会安全回退为用户确认。",
                "No local approval model is selected, so automatic approval safely falls back to user confirmation."),
            ApprovalReviewerReadinessState.ConnectorNotPaired => _localization.Text(
                "本机 Connector 尚未配对，自动审批会安全回退为用户确认。",
                "The local Connector is not paired, so automatic approval safely falls back to user confirmation."),
            _ => _localization.Text(
                "审批模型、Agent Prompt 或权限策略尚未就绪，自动审批会安全回退为用户确认。",
                "The approval model, Agent Prompt, or capability policy is not ready, so automatic approval safely falls back to user confirmation."),
        }
        : string.Empty;

    public async Task OpenAsync(CancellationToken cancellationToken = default)
    {
        await RunAsync(async token =>
        {
            await _coordinator.InitializeAsync(token).ConfigureAwait(false);
            await UpdateAsync(() =>
            {
                Mode = _coordinator.Mode;
                ApplyPending(_coordinator.Snapshot());
                ResetReviewerReadiness();
            }, token).ConfigureAwait(false);
            await Task.WhenAll(
                ReadHistoryCoreAsync(token, reportActionError: true),
                RefreshReviewerReadinessCoreAsync(token)).ConfigureAwait(false);
        }, cancellationToken).ConfigureAwait(false);
    }

    public Task SetModeAsync(
        ConnectorApprovalMode mode,
        bool riskConfirmed,
        CancellationToken cancellationToken = default) => RunAsync(async token =>
    {
        await _coordinator.SetModeAsync(mode, riskConfirmed, token).ConfigureAwait(false);
        await UpdateAsync(() =>
        {
            Mode = _coordinator.Mode;
            ResetReviewerReadiness();
            ActionMessage = _localization.Text(
                "审批模式已更新。",
                "The approval mode was updated.");
        }).ConfigureAwait(false);
        token.ThrowIfCancellationRequested();
        await RefreshReviewerReadinessCoreAsync(token).ConfigureAwait(false);
    }, cancellationToken);

    public Task RefreshReviewerReadinessAsync(CancellationToken cancellationToken = default) =>
        RunAsync(RefreshReviewerReadinessCoreAsync, cancellationToken, preserveMessages: true);

    private void ResetReviewerReadiness()
    {
        ReviewerReadinessErrorMessage = null;
        ReviewerReadinessState = ApprovalReviewerReadinessState.ManagedConfigurationInvalid;
        IsReviewerReadinessChecking = false;
    }

    private async Task RefreshReviewerReadinessCoreAsync(CancellationToken cancellationToken)
    {
        if (Mode != ConnectorApprovalMode.AutoApproval) return;
        await UpdateAsync(() =>
        {
            ResetReviewerReadiness();
            IsReviewerReadinessChecking = true;
        }, cancellationToken).ConfigureAwait(false);
        try
        {
            var readiness = await CheckReadinessAsync(cancellationToken).ConfigureAwait(false);
            await UpdateAsync(() => ReviewerReadinessState = readiness.State, cancellationToken).ConfigureAwait(false);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            await UpdateAsync(() => ReviewerReadinessErrorMessage = exception.Message, cancellationToken).ConfigureAwait(false);
        }
        finally
        {
            await UpdateAsync(() => IsReviewerReadinessChecking = false).ConfigureAwait(false);
        }
    }

    private Task<ApprovalReviewerReadiness> CheckReadinessAsync(CancellationToken cancellationToken) =>
        _reviewerReadiness?.CheckAsync(cancellationToken) ?? Task.FromResult(
            new ApprovalReviewerReadiness(ApprovalReviewerReadinessState.ManagedConfigurationInvalid));

    public Task ResolveAsync(
        ApprovalPendingItemViewModel item,
        ConnectorApprovalAction action,
        CancellationToken cancellationToken = default) => RunAsync(async token =>
    {
        if (!await _coordinator.ResolveAsync(item.Id, action, token).ConfigureAwait(false))
        {
            throw new InvalidOperationException(_localization.Text(
                "该审批已经被处理或不再有效。",
                "This approval was already resolved or is no longer active."));
        }

        await UpdateAsync(() =>
        {
            ApplyPending(_coordinator.Snapshot());
            ActionMessage = _localization.Text("审批已处理。", "The approval was resolved.");
        }, token).ConfigureAwait(false);
        await ReadEventHistoryAsync(token).ConfigureAwait(false);
    }, cancellationToken);

    public void Dispose()
    {
        lock (_lifecycleGate)
        {
            if (_disposed) return;
            _disposed = true;
        }
        _coordinator.PendingChanged -= OnPendingChanged;
        _coordinator.DecisionRecorded -= OnDecisionRecorded;
        _localization.PropertyChanged -= OnLocalizationChanged;
        try
        {
            _lifetime.Cancel();
        }
        finally
        {
            lock (_lifecycleGate)
            {
                _cancellationFinished = true;
                DisposeOperationsIfIdle();
            }
        }
    }

    private async Task RunAsync(
        Func<CancellationToken, Task> operation,
        CancellationToken cancellationToken,
        bool background = false,
        bool preserveMessages = false)
    {
        CancellationTokenSource linked;
        lock (_lifecycleGate)
        {
            if (_disposed) return;
            linked = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken, _lifetime.Token);
            _activeOperations++;
        }

        using (linked)
        {
            var token = linked.Token;
            try
            {
                await _operations.RunAsync(
                    async current =>
                    {
                        current.ThrowIfCancellationRequested();
                        await operation(current).ConfigureAwait(false);
                    },
                    () =>
                    {
                        if (_disposed || token.IsCancellationRequested || background) return;
                        IsBusy = true;
                        if (!preserveMessages)
                        {
                            ErrorMessage = null;
                            ActionMessage = null;
                        }
                    },
                    message =>
                    {
                        if (_disposed || token.IsCancellationRequested) return;
                        if (background) HistorySyncErrorMessage = message;
                        else ErrorMessage = message;
                    },
                    () =>
                    {
                        if (_disposed || background) return;
                        IsBusy = false;
                        ScheduleHistoryRefresh();
                    },
                    token).ConfigureAwait(false);
                token.ThrowIfCancellationRequested();
            }
            catch (OperationCanceledException) when (_disposed)
            {
                // Closing the view model cancels queued work without surfacing an action failure.
            }
            finally
            {
                lock (_lifecycleGate)
                {
                    _activeOperations--;
                    DisposeOperationsIfIdle();
                }
            }
        }
    }

    private void DisposeOperationsIfIdle()
    {
        if (!_cancellationFinished || _activeOperations != 0) return;
        _operations.Dispose();
        _lifetime.Dispose();
    }

    private Task UpdateAsync(Action action, CancellationToken cancellationToken = default) =>
        _dispatcher.InvokeAsync(() =>
        {
            if (!_disposed && !cancellationToken.IsCancellationRequested) action();
        }, cancellationToken);

    private void ScheduleHistoryRefresh()
    {
        if (_disposed || IsBusy || !_historyRefreshPending || IsHistoryRefreshing) return;
        _historyRefreshPending = false;
        IsHistoryRefreshing = true;
        _ = RefreshEventHistoryAsync();
    }

    private async Task RefreshEventHistoryAsync()
    {
        try
        {
            await RunAsync(ReadEventHistoryAsync, CancellationToken.None, background: true).ConfigureAwait(false);
        }
        catch
        {
            // A dispatcher that is shutting down cannot receive an error notification.
        }
        finally
        {
            try
            {
                await UpdateAsync(() =>
                {
                    IsHistoryRefreshing = false;
                    ScheduleHistoryRefresh();
                }).ConfigureAwait(false);
            }
            catch
            {
                // Do not let an unobserved event refresh crash application shutdown.
            }
        }
    }

    private Task ReadEventHistoryAsync(CancellationToken cancellationToken) => ReadHistoryCoreAsync(cancellationToken);

    private async Task ReadHistoryCoreAsync(CancellationToken cancellationToken, bool reportActionError = false)
    {
        var historyVersion = Interlocked.Read(ref _historyChangeVersion);
        try
        {
            var history = await _coordinator.ReadHistoryAsync(1_000, cancellationToken).ConfigureAwait(false);
            await UpdateAsync(() =>
            {
                if (historyVersion != Interlocked.Read(ref _historyChangeVersion)) return;
                _historyRefreshPending = false;
                ApplyHistory(history);
            }, cancellationToken).ConfigureAwait(false);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            await UpdateAsync(() =>
            {
                if (historyVersion != Interlocked.Read(ref _historyChangeVersion)) return;
                _historyRefreshPending = false;
                HistorySyncErrorMessage = exception.Message;
                if (reportActionError) ErrorMessage = exception.Message;
            }, cancellationToken).ConfigureAwait(false);
        }
    }

    private void ApplyPending(IReadOnlyList<ConnectorPendingApproval> pending)
    {
        Pending.Clear();
        foreach (var item in pending)
        {
            Pending.Add(new ApprovalPendingItemViewModel(item, _localization));
        }

        OnPropertyChanged(nameof(HasPending));
    }

    private void ApplyHistory(IReadOnlyList<ConnectorApprovalHistoryEntry> history)
    {
        HistorySyncErrorMessage = null;
        _history = history.Take(1_000).ToArray();
        ApplyHistoryPage();
    }

    private void ApplyHistoryPage()
    {
        _historyPageIndex = Math.Clamp(_historyPageIndex, 0, HistoryPageCount - 1);
        History.Clear();
        foreach (var item in _history.Skip(HistoryPageIndex * HistoryPageSize).Take(HistoryPageSize))
        {
            History.Add(new ApprovalHistoryItemViewModel(item, _localization));
        }

        OnPropertyChanged(nameof(HasHistory));
        OnPropertyChanged(nameof(HistoryCount));
        OnPropertyChanged(nameof(HistoryPageIndex));
        OnPropertyChanged(nameof(HistoryPageCount));
        OnPropertyChanged(nameof(HasHistoryPagination));
        OnPropertyChanged(nameof(HistoryRangeLabel));
        OnPropertyChanged(nameof(HistoryPageLabel));
        PreviousHistoryPageCommand.NotifyCanExecuteChanged();
        NextHistoryPageCommand.NotifyCanExecuteChanged();
    }

    private async void OnPendingChanged(object? sender, EventArgs e)
    {
        await ProcessApprovalNotificationAsync().ConfigureAwait(false);
    }

    private async void OnDecisionRecorded(object? sender, ConnectorApprovalDecisionEventArgs e) =>
        await ProcessApprovalNotificationAsync().ConfigureAwait(false);

    private async Task ProcessApprovalNotificationAsync()
    {
        Interlocked.Increment(ref _historyChangeVersion);
        try
        {
            await UpdateAsync(() =>
            {
                ApplyPending(_coordinator.Snapshot());
                _historyRefreshPending = true;
                ScheduleHistoryRefresh();
            }).ConfigureAwait(false);
        }
        catch
        {
            // Approval notifications must not escape async event handlers during shutdown.
        }
    }

    private async void OnLocalizationChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
    {
        try
        {
            await UpdateAsync(() =>
            {
                ApplyPending(_coordinator.Snapshot());
                ApplyHistoryPage();
                OnPropertyChanged(string.Empty);
            }).ConfigureAwait(false);
        }
        catch
        {
            // The dispatcher may already be unavailable while the application is closing.
        }
    }
}

public sealed class ApprovalPendingItemViewModel
{
    public ApprovalPendingItemViewModel(
        ConnectorPendingApproval approval,
        LocalizationViewModel localization)
    {
        Approval = approval;
        RiskLabel = approval.Risk.Level switch
        {
            ConnectorApprovalRiskLevel.High => localization.Text("高风险", "High risk"),
            ConnectorApprovalRiskLevel.Medium => localization.Text("需注意", "Caution"),
            _ => localization.Text("低风险", "Low risk"),
        };
        DeclineLabel = localization.Text("拒绝", "Decline");
        AcceptLabel = localization.Text("本次允许", "Allow once");
        AcceptForSessionLabel = localization.Text("本会话允许", "Allow for session");
    }

    public ConnectorPendingApproval Approval { get; }
    public string Id => Approval.Id;
    public string Command => Approval.Command;
    public string WorkingDirectory => Approval.WorkingDirectory;
    public string Source => Approval.Source;
    public string Reason => Approval.Reason ?? Approval.Risk.Reason ?? string.Empty;
    public string CreatedLabel => Approval.CreatedAt.ToLocalTime().ToString("yyyy-MM-dd HH:mm:ss");
    public string RiskLabel { get; }
    public string DeclineLabel { get; }
    public string AcceptLabel { get; }
    public string AcceptForSessionLabel { get; }
}

public sealed class ApprovalHistoryItemViewModel
{
    public ApprovalHistoryItemViewModel(
        ConnectorApprovalHistoryEntry entry,
        LocalizationViewModel localization)
    {
        Entry = entry;
        DecisionLabel = entry.Approved
            ? localization.Text("已允许", "Allowed")
            : localization.Text("已拒绝", "Declined");
        ReviewerLabel = entry.Reviewer switch
        {
            ConnectorApprovalReviewer.Ai => localization.Text("AI 审批", "AI reviewer"),
            ConnectorApprovalReviewer.Policy => localization.Text("策略", "Policy"),
            ConnectorApprovalReviewer.Session => localization.Text("会话授权", "Session grant"),
            ConnectorApprovalReviewer.System => localization.Text("系统", "System"),
            _ => localization.Text("用户", "User"),
        };
    }

    public ConnectorApprovalHistoryEntry Entry { get; }
    public string Command => Entry.Command;
    public string Source => Entry.Source;
    public string Reason => Entry.Reason;
    public string CreatedLabel => Entry.CreatedAt.ToLocalTime().ToString("yyyy-MM-dd HH:mm:ss");
    public string DecisionLabel { get; }
    public string ReviewerLabel { get; }
}
