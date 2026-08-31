using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Remote;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class RemoteConnectionsViewModelTests
{
    [Fact]
    public async Task OpenLoadsCloudConnectionsAndConnectorWorkspaces()
    {
        var service = new ServiceDouble();
        var viewModel = new RemoteConnectionsViewModel(service, new ConnectorDouble(), new ImmediateUiDispatcher());

        await viewModel.OpenAsync();

        Assert.Single(viewModel.Connections);
        Assert.Single(viewModel.Workspaces);
        Assert.Equal("workspace-1", viewModel.SelectedWorkspace?.Id);
        Assert.False(viewModel.CanSave);
    }

    [Fact]
    public async Task SaveCreatesConnectionBoundToSelectedConnectorWorkspace()
    {
        var service = new ServiceDouble();
        var viewModel = new RemoteConnectionsViewModel(service, new ConnectorDouble(), new ImmediateUiDispatcher());
        await viewModel.OpenAsync();
        viewModel.Host = "server.example";
        viewModel.Username = "deploy";
        viewModel.AuthenticationType = RemoteAuthenticationType.Password;
        viewModel.Password = "secret";

        await viewModel.SaveCommand.ExecuteAsync(null);

        Assert.Equal("device-1", service.LastDraft?.LocalConnectorDeviceId);
        Assert.Equal("workspace-1", service.LastDraft?.LocalConnectorWorkspaceId);
        Assert.Equal("remote-new", viewModel.SelectedConnection?.Id);
        Assert.Equal(string.Empty, viewModel.Password);
    }

    [Fact]
    public async Task TestSurfacesSecondFactorPromptAndRetriesWithCode()
    {
        var service = new ServiceDouble { RequireVerification = true };
        var viewModel = new RemoteConnectionsViewModel(service, new ConnectorDouble(), new ImmediateUiDispatcher());
        await viewModel.OpenAsync();
        viewModel.EditCommand.Execute(service.Values[0]);

        await viewModel.TestCommand.ExecuteAsync(null);
        Assert.Equal("Verification code:", viewModel.VerificationPrompt);

        service.RequireVerification = false;
        viewModel.VerificationCode = "123456";
        await viewModel.TestCommand.ExecuteAsync(null);

        Assert.Equal("123456", service.LastVerificationCode);
        Assert.Null(viewModel.VerificationPrompt);
        Assert.Equal("SSH ok", viewModel.ActionMessage);
    }

    [Fact]
    public async Task ExistingStoredCredentialAllowsEditingWithoutRevealingPassword()
    {
        var service = new ServiceDouble();
        var viewModel = new RemoteConnectionsViewModel(service, new ConnectorDouble(), new ImmediateUiDispatcher());
        await viewModel.OpenAsync();

        viewModel.EditCommand.Execute(service.Values[0]);

        Assert.Equal(string.Empty, viewModel.Password);
        Assert.True(viewModel.CanSave);
    }

    private sealed class ServiceDouble : IRemoteConnectionService
    {
        public List<RemoteConnection> Values { get; } = [Connection("remote-1")];
        public RemoteConnectionDraft? LastDraft { get; private set; }
        public string? LastVerificationCode { get; private set; }
        public bool RequireVerification { get; set; }

        public Task<IReadOnlyList<RemoteConnection>> ListAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<RemoteConnection>>(Values);

        public Task<RemoteConnection> CreateAsync(RemoteConnectionDraft draft, CancellationToken cancellationToken = default)
        {
            LastDraft = draft;
            var value = Connection("remote-new") with { Host = draft.Host, Username = draft.Username };
            Values.Add(value);
            return Task.FromResult(value);
        }

        public Task<RemoteConnection> UpdateAsync(string id, RemoteConnectionDraft draft, CancellationToken cancellationToken = default)
        {
            LastDraft = draft;
            return Task.FromResult(Connection(id));
        }

        public Task DeleteAsync(string id, CancellationToken cancellationToken = default)
        {
            Values.RemoveAll(value => value.Id == id);
            return Task.CompletedTask;
        }

        public Task<RemoteConnectionTestResult> TestDraftAsync(RemoteConnectionDraft draft, string? verificationCode, CancellationToken cancellationToken = default)
        {
            LastDraft = draft;
            LastVerificationCode = verificationCode;
            if (RequireVerification) throw new RemoteVerificationRequiredException("Verification code:");
            return Task.FromResult(new RemoteConnectionTestResult(true, "SSH ok"));
        }

        public Task<RemoteConnectionTestResult> TestSavedAsync(string id, string? verificationCode, CancellationToken cancellationToken = default) =>
            TestDraftAsync(LastDraft!, verificationCode, cancellationToken);

        private static RemoteConnection Connection(string id) => new(
            id, "Server", "server.example", 22, "deploy", RemoteAuthenticationType.Password,
            true, false, false, "/srv/app", RemoteHostKeyPolicy.Strict,
            "device-1", "workspace-1", false, null, null, null, null, false, false, false, null);
    }

    private sealed class ConnectorDouble : ILocalConnectorControlService
    {
        public Task<LocalConnectorStatus> GetStatusAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult(new LocalConnectorStatus(true, "Connected", "owner", "device-1", "PC",
                "https://gateway.example", null, null, null,
                [new LocalConnectorWorkspaceStatus("workspace-1", "Work", "C:\\Work")]));

        public Task<LocalConnectorStatus> PairAsync(LocalConnectorPairingDraft draft, string ticket, CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();
        public Task DisconnectAsync(CancellationToken cancellationToken = default) => throw new NotSupportedException();
    }
}
