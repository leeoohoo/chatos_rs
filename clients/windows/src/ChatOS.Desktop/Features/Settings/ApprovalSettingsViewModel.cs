using System.Collections.ObjectModel;
using ChatOS.Connector.Approval;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Desktop.Features.Settings;

public sealed partial class ApprovalSettingsViewModel : ObservableObject, IDisposable
{
    private readonly CommandApprovalCoordinator _coordinator;
    private readonly IApprovalReviewerReadinessService? _reviewerReadiness;
    private readonly LocalizationViewModel _localization;
    private readonly IUiDispatcher _dispatcher;
    private readonly SemaphoreSlim _operationGate = new(1, 1);

    public ApprovalSettingsViewModel(
        CommandApprovalCoordinator coordinator,
        LocalizationViewModel localization,
        IUiDispatcher dispatcher,
        IApprovalReviewerReadinessService? reviewerReadiness = null)
    {
        _coordinator = coordinator;
        _localization = localization;
        _dispatcher = dispatcher;
        _reviewerReadiness = reviewerReadiness;
        _coordinator.PendingChanged += OnPendingChanged;
        _localization.PropertyChanged += OnLocalizationChanged;
    }

    public ObservableCollection<ApprovalPendingItemViewModel> Pending { get; } = [];

    public ObservableCollection<ApprovalHistoryItemViewModel> History { get; } = [];

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(ModeDescription))]
    [NotifyPropertyChangedFor(nameof(AutomaticReviewerStatus))]
    private ConnectorApprovalMode _mode = ConnectorApprovalMode.RequestApproval;

    [ObservableProperty]
    private bool _isBusy;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private string? _actionMessage;

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
        ? ReviewerReadinessState switch
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
            var historyTask = _coordinator.ReadHistoryAsync(30, token);
            var readinessTask = CheckReadinessAsync(token);
            await Task.WhenAll(historyTask, readinessTask).ConfigureAwait(false);
            var pending = _coordinator.Snapshot();
            await _dispatcher.InvokeAsync(() =>
            {
                ReviewerReadinessState = readinessTask.Result.State;
                Apply(_coordinator.Mode, pending, historyTask.Result);
            }, token)
                .ConfigureAwait(false);
        }, cancellationToken).ConfigureAwait(false);
    }

    public Task SetModeAsync(
        ConnectorApprovalMode mode,
        bool riskConfirmed,
        CancellationToken cancellationToken = default) => RunAsync(async token =>
    {
        await _coordinator.SetModeAsync(mode, riskConfirmed, token).ConfigureAwait(false);
        var readiness = await CheckReadinessAsync(token).ConfigureAwait(false);
        await _dispatcher.InvokeAsync(() =>
        {
            Mode = _coordinator.Mode;
            ReviewerReadinessState = readiness.State;
            ActionMessage = _localization.Text(
                "审批模式已更新。",
                "The approval mode was updated.");
        }, token).ConfigureAwait(false);
    }, cancellationToken);

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

        var history = await _coordinator.ReadHistoryAsync(30, token).ConfigureAwait(false);
        await _dispatcher.InvokeAsync(() =>
        {
            ApplyPending(_coordinator.Snapshot());
            ApplyHistory(history);
            ActionMessage = _localization.Text("审批已处理。", "The approval was resolved.");
        }, token).ConfigureAwait(false);
    }, cancellationToken);

    public void Dispose()
    {
        _coordinator.PendingChanged -= OnPendingChanged;
        _localization.PropertyChanged -= OnLocalizationChanged;
        _operationGate.Dispose();
    }

    private async Task RunAsync(
        Func<CancellationToken, Task> operation,
        CancellationToken cancellationToken)
    {
        await _operationGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            await _dispatcher.InvokeAsync(() =>
            {
                IsBusy = true;
                ErrorMessage = null;
                ActionMessage = null;
            }, cancellationToken).ConfigureAwait(false);
            await operation(cancellationToken).ConfigureAwait(false);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() => IsBusy = false).ConfigureAwait(false);
            _operationGate.Release();
        }
    }

    private void Apply(
        ConnectorApprovalMode mode,
        IReadOnlyList<ConnectorPendingApproval> pending,
        IReadOnlyList<ConnectorApprovalHistoryEntry> history)
    {
        Mode = mode;
        ApplyPending(pending);
        ApplyHistory(history);
    }

    private void ApplyPending(IReadOnlyList<ConnectorPendingApproval> pending)
    {
        Pending.Clear();
        foreach (var item in pending)
        {
            Pending.Add(new ApprovalPendingItemViewModel(item, _localization));
        }
    }

    private void ApplyHistory(IReadOnlyList<ConnectorApprovalHistoryEntry> history)
    {
        History.Clear();
        foreach (var item in history.Take(30))
        {
            History.Add(new ApprovalHistoryItemViewModel(item, _localization));
        }
    }

    private async void OnPendingChanged(object? sender, EventArgs e)
    {
        var pending = _coordinator.Snapshot();
        await _dispatcher.InvokeAsync(() => ApplyPending(pending));
    }

    private async void OnLocalizationChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
    {
        var pending = _coordinator.Snapshot();
        IReadOnlyList<ConnectorApprovalHistoryEntry> history;
        try
        {
            history = await _coordinator.ReadHistoryAsync(30).ConfigureAwait(false);
        }
        catch
        {
            history = History.Select(value => value.Entry).ToArray();
        }

        await _dispatcher.InvokeAsync(() =>
        {
            ApplyPending(pending);
            ApplyHistory(history);
            OnPropertyChanged(string.Empty);
        });
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
