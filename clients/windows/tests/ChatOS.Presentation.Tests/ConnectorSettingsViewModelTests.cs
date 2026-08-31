using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class ConnectorSettingsViewModelTests
{
    [Fact]
    public async Task PairRequestsTicketAndSendsNormalizedDraft()
    {
        var control = new ControlDouble();
        var tickets = new TicketDouble();
        using var viewModel = new ConnectorSettingsViewModel(
            control,
            tickets,
            new ImmediateUiDispatcher(),
            TimeSpan.FromMinutes(1));
        await viewModel.OpenAsync();
        viewModel.GatewayBaseUrl = " https://gateway.example ";
        viewModel.DeviceName = " Windows PC ";
        viewModel.Workspaces.Add(new LocalConnectorWorkspaceDraft("C:\\Work", "Work"));

        await viewModel.PairCommand.ExecuteAsync(null);

        Assert.Equal(1, tickets.IssueCount);
        Assert.Equal("ticket-1", control.LastTicket);
        Assert.Equal("https://gateway.example", control.LastPairing?.GatewayBaseUrl);
        Assert.Equal("Windows PC", control.LastPairing?.DeviceName);
        Assert.Equal("C:\\Work", control.LastPairing?.Workspaces.Single().AbsoluteRoot);
        Assert.True(viewModel.IsPaired);
        Assert.Empty(viewModel.Workspaces);
        Assert.False(viewModel.IsBusy);
    }

    [Fact]
    public void WorkspaceCommandIgnoresDuplicateRoots()
    {
        using var viewModel = new ConnectorSettingsViewModel(
            new ControlDouble(),
            new TicketDouble(),
            new ImmediateUiDispatcher());

        viewModel.AddWorkspaceCommand.Execute(new LocalConnectorWorkspaceDraft("C:\\Work"));
        viewModel.AddWorkspaceCommand.Execute(new LocalConnectorWorkspaceDraft("C:\\Work"));

        Assert.Single(viewModel.Workspaces);
    }

    [Fact]
    public async Task DisconnectRefreshesAuthoritativeStatus()
    {
        var control = new ControlDouble { Current = PairedStatus() };
        using var viewModel = new ConnectorSettingsViewModel(
            control,
            new TicketDouble(),
            new ImmediateUiDispatcher(),
            TimeSpan.FromMinutes(1));
        await viewModel.OpenAsync();

        await viewModel.DisconnectCommand.ExecuteAsync(null);

        Assert.Equal(1, control.DisconnectCount);
        Assert.False(viewModel.IsPaired);
        Assert.Equal("本机 Connector 配对已清除。", viewModel.ActionMessage);
    }

    [Fact]
    public async Task MonitorRefreshesStatusUntilClosed()
    {
        var control = new ControlDouble();
        using var viewModel = new ConnectorSettingsViewModel(
            control,
            new TicketDouble(),
            new ImmediateUiDispatcher(),
            TimeSpan.FromMilliseconds(10));

        await viewModel.OpenAsync();
        await control.SecondRead.Task.WaitAsync(TimeSpan.FromSeconds(1));
        viewModel.Close();
        var readsAfterClose = control.ReadCount;
        await Task.Delay(40);

        Assert.True(readsAfterClose >= 2);
        Assert.Equal(readsAfterClose, control.ReadCount);
    }

    private static LocalConnectorStatus PairedStatus() => new(
        true,
        "Connected",
        "owner",
        "device-1",
        "Windows PC",
        "https://gateway.example",
        DateTimeOffset.UtcNow,
        DateTimeOffset.UtcNow,
        null,
        [new LocalConnectorWorkspaceStatus("workspace-1", "Work", "C:\\Work")]);

    private sealed class TicketDouble : ILocalConnectorPairingTicketService
    {
        public int IssueCount { get; private set; }

        public Task<string> IssueAsync(CancellationToken cancellationToken = default)
        {
            IssueCount++;
            return Task.FromResult("ticket-1");
        }
    }

    private sealed class ControlDouble : ILocalConnectorControlService
    {
        private int _readCount;

        public LocalConnectorStatus Current { get; set; } = new(
            false, "Unconfigured", null, null, null, null, null, null, null, []);

        public int ReadCount => Volatile.Read(ref _readCount);

        public TaskCompletionSource SecondRead { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public LocalConnectorPairingDraft? LastPairing { get; private set; }

        public string? LastTicket { get; private set; }

        public int DisconnectCount { get; private set; }

        public Task<LocalConnectorStatus> GetStatusAsync(CancellationToken cancellationToken = default)
        {
            if (Interlocked.Increment(ref _readCount) >= 2) SecondRead.TrySetResult();
            return Task.FromResult(Current);
        }

        public Task<LocalConnectorStatus> PairAsync(
            LocalConnectorPairingDraft draft,
            string ticket,
            CancellationToken cancellationToken = default)
        {
            LastPairing = draft;
            LastTicket = ticket;
            Current = PairedStatus();
            return Task.FromResult(Current);
        }

        public Task DisconnectAsync(CancellationToken cancellationToken = default)
        {
            DisconnectCount++;
            Current = new LocalConnectorStatus(
                false, "Unconfigured", null, null, null, null, null, null, null, []);
            return Task.CompletedTask;
        }
    }
}
