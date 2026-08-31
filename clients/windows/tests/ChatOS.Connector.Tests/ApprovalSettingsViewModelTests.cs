using ChatOS.Connector.Approval;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Desktop.Features.Settings;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;

namespace ChatOS.Connector.Tests;

public sealed class ApprovalSettingsViewModelTests
{
    [Fact]
    public async Task LoadsPersistedModeAndRecentHistory()
    {
        var store = new MemoryApprovalStore { Mode = ConnectorApprovalMode.AutoApproval };
        store.History.Add(History("history-1", approved: true));
        var viewModel = Create(store, out _);

        await viewModel.OpenAsync();

        Assert.Equal(ConnectorApprovalMode.AutoApproval, viewModel.Mode);
        Assert.Single(viewModel.History);
        Assert.Contains("安全回退", viewModel.AutomaticReviewerStatus);
    }

    [Fact]
    public async Task FullControlRequiresExplicitConfirmationAndPreservesPreviousModeOnFailure()
    {
        var store = new MemoryApprovalStore();
        var viewModel = Create(store, out _);
        await viewModel.OpenAsync();

        await viewModel.SetModeAsync(ConnectorApprovalMode.FullControl, riskConfirmed: false);

        Assert.Equal(ConnectorApprovalMode.RequestApproval, viewModel.Mode);
        Assert.NotNull(viewModel.ErrorMessage);

        await viewModel.SetModeAsync(ConnectorApprovalMode.FullControl, riskConfirmed: true);

        Assert.Equal(ConnectorApprovalMode.FullControl, viewModel.Mode);
        Assert.Equal(ConnectorApprovalMode.FullControl, store.Mode);
    }

    [Fact]
    public async Task PendingApprovalCanBeResolvedInSettingsAndMovesToHistory()
    {
        var store = new MemoryApprovalStore();
        var viewModel = Create(store, out var coordinator);
        await viewModel.OpenAsync();
        var request = coordinator.RequestAsync(Request(), new ConnectorApprovalRisk(
            ConnectorApprovalRiskLevel.High,
            "destructive"));
        await WaitUntilAsync(() => viewModel.Pending.Count == 1);

        await viewModel.ResolveAsync(viewModel.Pending[0], ConnectorApprovalAction.Decline);

        Assert.Empty(viewModel.Pending);
        Assert.False((await request).Approved);
        Assert.Single(viewModel.History);
        Assert.Equal("已拒绝", viewModel.History[0].DecisionLabel);
    }

    private static ApprovalSettingsViewModel Create(
        MemoryApprovalStore store,
        out CommandApprovalCoordinator coordinator)
    {
        var dispatcher = new ImmediateUiDispatcher();
        var preferences = new AppPreferencesManager(new MemoryPreferencesStore());
        var localization = new LocalizationViewModel(preferences, dispatcher);
        coordinator = new CommandApprovalCoordinator(store);
        return new ApprovalSettingsViewModel(coordinator, localization, dispatcher);
    }

    private static CommandApprovalRequest Request() => new(
        "request-1",
        "owner",
        "device",
        "workspace",
        "git",
        ["reset", "--hard"],
        "C:\\workspace",
        "terminal",
        "scope");

    private static ConnectorApprovalHistoryEntry History(string id, bool approved) => new(
        id,
        "approval",
        "request",
        "workspace",
        "git status",
        "C:\\workspace",
        "terminal",
        ConnectorApprovalMode.RequestApproval,
        approved,
        ConnectorApprovalReviewer.User,
        ConnectorApprovalRiskLevel.Low,
        null,
        approved ? "approved" : "declined",
        DateTimeOffset.UnixEpoch);

    private static async Task WaitUntilAsync(Func<bool> condition)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(2));
        while (!condition()) await Task.Delay(5, timeout.Token);
    }

    private sealed class MemoryApprovalStore : IConnectorApprovalStore
    {
        public ConnectorApprovalMode? Mode { get; set; }
        public List<ConnectorApprovalHistoryEntry> History { get; } = [];

        public Task<ConnectorApprovalMode?> ReadModeAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult(Mode);

        public Task SaveModeAsync(
            ConnectorApprovalMode mode,
            CancellationToken cancellationToken = default)
        {
            Mode = mode;
            return Task.CompletedTask;
        }

        public Task AppendAsync(
            ConnectorApprovalHistoryEntry entry,
            CancellationToken cancellationToken = default)
        {
            History.Insert(0, entry);
            return Task.CompletedTask;
        }

        public Task<IReadOnlyList<ConnectorApprovalHistoryEntry>> ReadHistoryAsync(
            int limit = 1_000,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<ConnectorApprovalHistoryEntry>>(History.Take(limit).ToArray());
    }

    private sealed class MemoryPreferencesStore : IAppPreferencesStore
    {
        public Task<AppPreferences?> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult<AppPreferences?>(AppPreferences.Default);

        public Task SaveAsync(
            AppPreferences preferences,
            CancellationToken cancellationToken = default) => Task.CompletedTask;
    }
}
