using ChatOS.Connector.Approval;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Desktop.Features.Settings;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;

namespace ChatOS.Connector.Tests;

public sealed partial class ApprovalSettingsViewModelTests
{
    [Fact]
    public async Task SavedAutomaticModeIsVisibleBeforeReadinessCompletes()
    {
        var store = new MemoryApprovalStore();
        var pending = new TaskCompletionSource<ApprovalReviewerReadiness>(TaskCreationOptions.RunContinuationsAsynchronously);
        using var viewModel = CreateWithReadiness(store, new ReadinessDouble(_ => pending.Task));
        var saving = viewModel.SetModeAsync(ConnectorApprovalMode.AutoApproval, true);
        try
        {
            Assert.Equal(ConnectorApprovalMode.AutoApproval, store.Mode);
            Assert.Equal(ConnectorApprovalMode.AutoApproval, viewModel.Mode);
            Assert.Equal("审批模式已更新。", viewModel.ActionMessage);
        }
        finally
        {
            pending.TrySetResult(new(ApprovalReviewerReadinessState.Ready));
            await saving;
        }
    }

    [Fact]
    public async Task SavedModeStaysVisibleAfterReadinessThrows()
    {
        var store = new MemoryApprovalStore();
        using var viewModel = CreateWithReadiness(store, new ReadinessDouble(_ =>
            Task.FromException<ApprovalReviewerReadiness>(new IOException("readiness unavailable"))));

        await viewModel.SetModeAsync(ConnectorApprovalMode.AutoApproval, true);

        Assert.Equal(ConnectorApprovalMode.AutoApproval, store.Mode);
        Assert.Equal(ConnectorApprovalMode.AutoApproval, viewModel.Mode);
        Assert.Equal("审批模式已更新。", viewModel.ActionMessage);
        Assert.Null(viewModel.ErrorMessage);
        Assert.Equal("readiness unavailable", viewModel.ReviewerReadinessErrorMessage);
        Assert.True(viewModel.HasReviewerReadinessError);
        Assert.DoesNotContain("已就绪", viewModel.AutomaticReviewerStatus);
        Assert.False(viewModel.IsReviewerReadinessChecking);
    }

    [Fact]
    public async Task OpenShowsModeAndHistoryWhileReadinessIsPending()
    {
        var store = HistoryStore(1);
        store.Mode = ConnectorApprovalMode.AutoApproval;
        var pending = new TaskCompletionSource<ApprovalReviewerReadiness>(TaskCreationOptions.RunContinuationsAsynchronously);
        using var viewModel = CreateWithReadiness(store, new ReadinessDouble(_ => pending.Task));
        var opening = viewModel.OpenAsync();
        try
        {
            Assert.Equal(ConnectorApprovalMode.AutoApproval, viewModel.Mode);
            Assert.Single(viewModel.History);
        }
        finally
        {
            pending.TrySetResult(new(ApprovalReviewerReadinessState.Ready));
            await opening;
        }
    }

    [Fact]
    public async Task HistoryReadFailureDoesNotHideLoadedApprovalMode()
    {
        var store = new MemoryApprovalStore
        {
            Mode = ConnectorApprovalMode.FullControl,
            HistoryReadError = new IOException("history unavailable"),
        };
        using var viewModel = CreateWithReadiness(store, new ReadinessDouble(_ =>
            Task.FromResult(new ApprovalReviewerReadiness(ApprovalReviewerReadinessState.Ready))));

        await viewModel.OpenAsync();

        Assert.Equal(ConnectorApprovalMode.FullControl, viewModel.Mode);
        Assert.Equal("history unavailable", viewModel.ErrorMessage);
        Assert.False(viewModel.IsBusy);
    }

    [Theory]
    [InlineData(ConnectorApprovalMode.RequestApproval)]
    [InlineData(ConnectorApprovalMode.FullControl)]
    public async Task NonAutomaticModesDoNotDependOnApprovalModelCheck(ConnectorApprovalMode mode)
    {
        var readiness = new ReadinessDouble(_ => throw new IOException("must not check"));
        var store = new MemoryApprovalStore();
        using var viewModel = CreateWithReadiness(store, readiness);

        await viewModel.SetModeAsync(mode, true);
        await viewModel.OpenAsync();

        Assert.Equal(mode, viewModel.Mode);
        Assert.Equal(0, readiness.Calls);
        Assert.Null(viewModel.ErrorMessage);
        Assert.False(viewModel.HasReviewerReadinessError);
        Assert.False(viewModel.CanRetryReviewerReadiness);
        Assert.Empty(viewModel.AutomaticReviewerStatus);
    }

    [Fact]
    public async Task ReadinessRetryPreservesSavedResultAndDoesNotSavePolicyOrReloadHistory()
    {
        var store = new MemoryApprovalStore();
        var readiness = new ReadinessDouble(_ => Task.FromException<ApprovalReviewerReadiness>(new IOException("offline")));
        using var viewModel = CreateWithReadiness(store, readiness);
        await viewModel.SetModeAsync(ConnectorApprovalMode.AutoApproval, true);
        viewModel.ErrorMessage = "another action failed";
        readiness.Check = _ => Task.FromResult(new ApprovalReviewerReadiness(ApprovalReviewerReadinessState.Ready));

        await viewModel.RefreshReviewerReadinessAsync();

        Assert.Equal(1, store.ModeSaveCount);
        Assert.Equal(0, store.HistoryReadCount);
        Assert.Equal(2, readiness.Calls);
        Assert.Equal(ConnectorApprovalMode.AutoApproval, viewModel.Mode);
        Assert.Equal("审批模式已更新。", viewModel.ActionMessage);
        Assert.Equal("another action failed", viewModel.ErrorMessage);
        Assert.False(viewModel.HasReviewerReadinessError);
        Assert.Contains("已就绪", viewModel.AutomaticReviewerStatus);
        Assert.True(viewModel.CanRetryReviewerReadiness);
    }

    [Theory]
    [InlineData(ApprovalReviewerReadinessState.Ready, "已就绪")]
    [InlineData(ApprovalReviewerReadinessState.ModelNotSelected, "尚未选择")]
    [InlineData(ApprovalReviewerReadinessState.ConnectorNotPaired, "尚未配对")]
    [InlineData(ApprovalReviewerReadinessState.ManagedConfigurationInvalid, "尚未就绪")]
    public async Task KnownReadinessStateClearsPreviousCheckError(ApprovalReviewerReadinessState state, string status)
    {
        var store = new MemoryApprovalStore { Mode = ConnectorApprovalMode.AutoApproval };
        var readiness = new ReadinessDouble(_ => Task.FromException<ApprovalReviewerReadiness>(new IOException("offline")));
        using var viewModel = CreateWithReadiness(store, readiness);
        await viewModel.OpenAsync();
        Assert.True(viewModel.HasReviewerReadinessError);
        readiness.Check = _ => Task.FromResult(new ApprovalReviewerReadiness(state));

        await viewModel.RefreshReviewerReadinessAsync();

        Assert.Equal(state, viewModel.ReviewerReadinessState);
        Assert.Contains(status, viewModel.AutomaticReviewerStatus);
        Assert.False(viewModel.HasReviewerReadinessError);
        Assert.False(viewModel.IsReviewerReadinessChecking);
        Assert.Equal(0, store.ModeSaveCount);
    }

    [Fact]
    public async Task RecheckingReadyModelDoesNotShowStaleReadyStatus()
    {
        var store = new MemoryApprovalStore { Mode = ConnectorApprovalMode.AutoApproval };
        var readiness = new ReadinessDouble(_ => Task.FromResult(new ApprovalReviewerReadiness(ApprovalReviewerReadinessState.Ready)));
        using var viewModel = CreateWithReadiness(store, readiness);
        await viewModel.OpenAsync();
        var pending = new TaskCompletionSource<ApprovalReviewerReadiness>(TaskCreationOptions.RunContinuationsAsynchronously);
        readiness.Check = _ => pending.Task;
        var checking = viewModel.RefreshReviewerReadinessAsync();
        try
        {
            Assert.True(viewModel.IsReviewerReadinessChecking);
            Assert.Contains("正在检查", viewModel.AutomaticReviewerStatus);
            Assert.DoesNotContain("已就绪", viewModel.AutomaticReviewerStatus);
            Assert.False(viewModel.CanRetryReviewerReadiness);
        }
        finally
        {
            pending.SetResult(new(ApprovalReviewerReadinessState.ModelNotSelected));
            await checking;
        }
        Assert.False(viewModel.IsReviewerReadinessChecking);
        Assert.Contains("尚未选择", viewModel.AutomaticReviewerStatus);
    }

    [Fact]
    public async Task PolicySaveFailureKeepsPreviousModeAndDoesNotCheckReadinessAgain()
    {
        var store = new MemoryApprovalStore { Mode = ConnectorApprovalMode.AutoApproval };
        var readiness = new ReadinessDouble(_ => Task.FromResult(new ApprovalReviewerReadiness(ApprovalReviewerReadinessState.Ready)));
        using var viewModel = CreateWithReadiness(store, readiness);
        await viewModel.OpenAsync();
        store.ModeSaveError = new IOException("cannot save policy");

        await viewModel.SetModeAsync(ConnectorApprovalMode.FullControl, true);

        Assert.Equal(ConnectorApprovalMode.AutoApproval, viewModel.Mode);
        Assert.Equal(ConnectorApprovalMode.AutoApproval, store.Mode);
        Assert.Equal("cannot save policy", viewModel.ErrorMessage);
        Assert.Null(viewModel.ActionMessage);
        Assert.Equal(1, readiness.Calls);
        Assert.Contains("已就绪", viewModel.AutomaticReviewerStatus);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task CancellationDuringReadinessKeepsCommittedModeAndRejectsLateResult(bool fail)
    {
        var pending = new TaskCompletionSource<ApprovalReviewerReadiness>(TaskCreationOptions.RunContinuationsAsynchronously);
        var store = new MemoryApprovalStore();
        using var viewModel = CreateWithReadiness(store, new ReadinessDouble(_ => pending.Task));
        using var cancellation = new CancellationTokenSource();
        var saving = viewModel.SetModeAsync(ConnectorApprovalMode.AutoApproval, true, cancellation.Token);

        cancellation.Cancel();
        if (fail) pending.SetException(new IOException("late check failure"));
        else pending.SetResult(new(ApprovalReviewerReadinessState.Ready));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => saving);

        Assert.Equal(ConnectorApprovalMode.AutoApproval, viewModel.Mode);
        Assert.Equal(ConnectorApprovalMode.AutoApproval, store.Mode);
        Assert.Equal("审批模式已更新。", viewModel.ActionMessage);
        Assert.False(viewModel.IsReviewerReadinessChecking);
        Assert.DoesNotContain("已就绪", viewModel.AutomaticReviewerStatus);
        Assert.Null(viewModel.ErrorMessage);
        Assert.False(viewModel.HasReviewerReadinessError);
    }

    [Fact]
    public async Task CancellationImmediatelyAfterPolicyCommitStillShowsActualSavedMode()
    {
        var store = new MemoryApprovalStore();
        var readiness = new ReadinessDouble(_ => throw new InvalidOperationException("must not check"));
        using var viewModel = CreateWithReadiness(store, readiness);
        using var cancellation = new CancellationTokenSource();
        store.ModeSaved = cancellation.Cancel;

        await Assert.ThrowsAnyAsync<OperationCanceledException>(() =>
            viewModel.SetModeAsync(ConnectorApprovalMode.AutoApproval, true, cancellation.Token));

        Assert.Equal(ConnectorApprovalMode.AutoApproval, viewModel.Mode);
        Assert.Equal(ConnectorApprovalMode.AutoApproval, store.Mode);
        Assert.Equal("审批模式已更新。", viewModel.ActionMessage);
        Assert.Equal(0, readiness.Calls);
        Assert.False(viewModel.IsBusy);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task DisposedViewModelIgnoresLateReadinessSuccessAndFailure(bool fail)
    {
        var store = new MemoryApprovalStore();
        var pending = new TaskCompletionSource<ApprovalReviewerReadiness>(TaskCreationOptions.RunContinuationsAsynchronously);
        CancellationToken observed = default;
        var viewModel = CreateWithReadiness(store, new ReadinessDouble(token => { observed = token; return pending.Task; }));
        var saving = viewModel.SetModeAsync(ConnectorApprovalMode.AutoApproval, true);
        var changes = 0;
        viewModel.PropertyChanged += (_, _) => changes++;

        viewModel.Dispose();
        Assert.True(observed.IsCancellationRequested);
        if (fail) pending.SetException(new IOException("late failure"));
        else pending.SetResult(new(ApprovalReviewerReadinessState.Ready));
        await saving;

        Assert.Equal(0, changes);
        Assert.DoesNotContain("已就绪", viewModel.AutomaticReviewerStatus);
    }

    [Fact]
    public async Task ReadinessChecksAreSerialAndQueuedRetryCanApplyNewerState()
    {
        var store = new MemoryApprovalStore();
        var pending = new TaskCompletionSource<ApprovalReviewerReadiness>(TaskCreationOptions.RunContinuationsAsynchronously);
        var calls = 0;
        var active = 0;
        var peak = 0;
        var readiness = new ReadinessDouble(async _ =>
        {
            peak = Math.Max(peak, Interlocked.Increment(ref active));
            try
            {
                return Interlocked.Increment(ref calls) == 1
                    ? await pending.Task : new ApprovalReviewerReadiness(ApprovalReviewerReadinessState.ModelNotSelected);
            }
            finally { Interlocked.Decrement(ref active); }
        });
        using var viewModel = CreateWithReadiness(store, readiness);
        var saving = viewModel.SetModeAsync(ConnectorApprovalMode.AutoApproval, true);
        var retrying = viewModel.RefreshReviewerReadinessAsync();
        Assert.Equal(1, calls);

        pending.SetResult(new(ApprovalReviewerReadinessState.Ready));
        await Task.WhenAll(saving, retrying);

        Assert.Equal(1, peak);
        Assert.Equal(2, calls);
        Assert.Equal(1, store.ModeSaveCount);
        Assert.Equal(ApprovalReviewerReadinessState.ModelNotSelected, viewModel.ReviewerReadinessState);
        Assert.Equal("审批模式已更新。", viewModel.ActionMessage);
    }

    [Fact]
    public async Task ReadinessAndHistoryFailuresAreIndependentAndReadableInEnglish()
    {
        var store = HistoryStore(1);
        store.Mode = ConnectorApprovalMode.AutoApproval;
        store.HistoryReadError = new IOException("history failed");
        var readiness = new ReadinessDouble(_ => Task.FromException<ApprovalReviewerReadiness>(new IOException("check failed")));
        var preferences = new AppPreferencesManager(new MemoryPreferencesStore());
        using var viewModel = CreateWithReadiness(store, readiness, preferences);
        await viewModel.OpenAsync();
        Assert.Equal("history failed", viewModel.ErrorMessage);
        Assert.Equal("check failed", viewModel.ReviewerReadinessErrorMessage);
        Assert.Equal(ConnectorApprovalMode.AutoApproval, viewModel.Mode);

        await preferences.UpdateAsync(value => value with { Language = InterfaceLanguage.English });
        Assert.Contains("unknown", viewModel.AutomaticReviewerStatus);
        Assert.Contains("current approval mode remains in effect", viewModel.ReviewerReadinessErrorDescription);
        Assert.Equal("Check approval model again", viewModel.RetryReviewerReadinessLabel);

        readiness.Check = _ => Task.FromResult(new ApprovalReviewerReadiness(ApprovalReviewerReadinessState.Ready));
        await viewModel.RefreshReviewerReadinessAsync();
        Assert.Equal("history failed", viewModel.ErrorMessage);
        Assert.True(viewModel.HasHistorySyncError);
        Assert.False(viewModel.HasReviewerReadinessError);
        Assert.Equal(1, store.HistoryReadCount);
    }

    [Fact]
    public async Task ForegroundDecisionReadConsumesItsNotificationWithoutRedundantRefresh()
    {
        var store = new MemoryApprovalStore();
        using var viewModel = Create(store, out var coordinator);
        await viewModel.OpenAsync();
        var pending = coordinator.RequestAsync(Request(), new ConnectorApprovalRisk(ConnectorApprovalRiskLevel.High));
        await WaitUntilAsync(() => viewModel.Pending.Count == 1 && !viewModel.IsHistoryRefreshing);
        var reads = store.HistoryReadCount;

        await viewModel.ResolveAsync(viewModel.Pending[0], ConnectorApprovalAction.Decline);

        Assert.False((await pending).Approved);
        Assert.Equal(reads + 1, store.HistoryReadCount);
        Assert.False(viewModel.IsHistoryRefreshing);
        Assert.Single(viewModel.History);
    }

    [Fact]
    public async Task ReadyStateCanBeAppliedBeforeHistoryReadFinishes()
    {
        var store = new MemoryApprovalStore { Mode = ConnectorApprovalMode.AutoApproval };
        var pending = new TaskCompletionSource<IReadOnlyList<ConnectorApprovalHistoryEntry>>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        store.HistoryReadHandler = _ => pending.Task;
        var readiness = new ReadinessDouble(_ => Task.FromResult(new ApprovalReviewerReadiness(ApprovalReviewerReadinessState.Ready)));
        using var viewModel = CreateWithReadiness(store, readiness);
        var opening = viewModel.OpenAsync();
        try
        {
            Assert.Equal(ApprovalReviewerReadinessState.Ready, viewModel.ReviewerReadinessState);
            Assert.Contains("已就绪", viewModel.AutomaticReviewerStatus);
            Assert.Equal(ConnectorApprovalMode.AutoApproval, viewModel.Mode);
        }
        finally
        {
            pending.SetException(new IOException("history failure"));
            await opening;
        }
        Assert.Contains("已就绪", viewModel.AutomaticReviewerStatus);
        Assert.Equal("history failure", viewModel.ErrorMessage);
        Assert.False(viewModel.HasReviewerReadinessError);
    }

    private static ApprovalSettingsViewModel CreateWithReadiness(MemoryApprovalStore store, ReadinessDouble readiness,
        AppPreferencesManager? preferences = null)
    {
        var dispatcher = new ImmediateUiDispatcher();
        return new(new CommandApprovalCoordinator(store),
            new LocalizationViewModel(preferences ?? new AppPreferencesManager(new MemoryPreferencesStore()), dispatcher),
            dispatcher, readiness);
    }

    private sealed class ReadinessDouble(Func<CancellationToken, Task<ApprovalReviewerReadiness>> check)
        : IApprovalReviewerReadinessService
    {
        public int Calls { get; private set; }
        public Func<CancellationToken, Task<ApprovalReviewerReadiness>> Check { get; set; } = check;
        public Task<ApprovalReviewerReadiness> CheckAsync(CancellationToken cancellationToken = default)
        {
            Calls++;
            return Check(cancellationToken);
        }
    }
}
