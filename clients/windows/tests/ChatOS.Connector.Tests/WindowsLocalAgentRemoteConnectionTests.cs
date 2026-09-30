using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentRemoteConnectionTests
{
    [Fact]
    public async Task MetadataLifecycleUsesOwnerAndOptimisticVersions()
    {
        var host = new RemoteHost();
        var service = new WindowsLocalAgentRemoteConnectionMetadataService(
            new WindowsLocalAgentRemoteConnectionClient(host));
        service.Configure("user-1");

        var created = await service.CreateAsync(Draft("Production"));
        Assert.Equal("remote-1", created.Id);
        var create = Assert.IsType<LocalRemoteCreateCommand>(host.LastCommand);
        Assert.Equal("user-1", create.OwnerUserId);

        var updated = await service.UpdateAsync(created.Id, Draft("Renamed"));
        Assert.Equal("Renamed", updated.Name);
        var update = Assert.IsType<LocalRemoteUpdateCommand>(host.LastCommand);
        Assert.Equal((ulong)1, update.ExpectedVersion);

        await service.DeleteAsync(created.Id);
        var delete = Assert.IsType<LocalRemoteDeleteCommand>(host.LastCommand);
        Assert.Equal((ulong)2, delete.ExpectedVersion);
    }

    private static RemoteConnectionDraft Draft(string name) => new(
        name,
        "server.example.com",
        22,
        "deploy",
        RemoteAuthenticationType.Password,
        "top-secret",
        null,
        null,
        "/srv/app",
        RemoteHostKeyPolicy.Strict,
        "device-local",
        "workspace-local",
        false,
        null,
        null,
        null,
        null,
        null,
        null,
        null);

    private sealed class RemoteHost : ILocalAgentHostClient
    {
        private ulong _version = 1;
        private string _name = "Production";

        public object? LastCommand { get; private set; }
        public string? ActiveOwnerUserId => "user-1";

        public Task StartForOwnerAsync(
            string ownerUserId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task RestartForOwnerAsync(
            string ownerUserId,
            IReadOnlyDictionary<string, string> credentialEnvironment,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task StopAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task<TResponse> SendAsync<TCommand, TResponse>(
            TCommand command,
            CancellationToken cancellationToken = default)
            where TCommand : notnull
        {
            LastCommand = command;
            object response = command switch
            {
                LocalRemoteCreateCommand => ConnectionResult(),
                LocalRemoteIdentityCommand => ConnectionResult(),
                LocalRemoteUpdateCommand => UpdateResult(),
                LocalRemoteDeleteCommand => new LocalRemoteConnectionDeletedResult(
                    "remote_connection_deleted",
                    "remote-1"),
                LocalRemoteOwnerCommand => new LocalRemoteConnectionsResult(
                    "remote_connections",
                    []),
                _ => throw new InvalidOperationException(
                    $"Unexpected remote command: {typeof(TCommand).Name}"),
            };
            return Task.FromResult((TResponse)response);
        }

        private LocalRemoteConnectionResult UpdateResult()
        {
            _version += 1;
            _name = "Renamed";
            return ConnectionResult();
        }

        private LocalRemoteConnectionResult ConnectionResult() => new(
            "remote_connection",
            new WindowsLocalRemoteConnection(
                "remote-1",
                "user-1",
                _name,
                "server.example.com",
                22,
                "deploy",
                "password",
                false,
                false,
                false,
                "/srv/app",
                "strict",
                "device-local",
                "workspace-local",
                false,
                null,
                null,
                null,
                null,
                false,
                false,
                false,
                null,
                _version,
                1,
                2));
    }
}
