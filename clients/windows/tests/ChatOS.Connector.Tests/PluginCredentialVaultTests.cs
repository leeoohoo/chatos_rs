using ChatOS.Connector.Persistence;
using ChatOS.Connector.Plugins;
using ChatOS.Connector.Security;

namespace ChatOS.Connector.Tests;

public sealed class PluginCredentialVaultTests : IDisposable
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(),
        $"chatos-plugin-credentials-{Guid.NewGuid():N}");

    [Fact]
    public async Task PersistsOnlyMetadataInSqliteAndSecretInCredentialStore()
    {
        var database = new LocalStateDatabase(Path.Combine(_directory, "state.db"));
        await database.InitializeAsync();
        var secrets = new MemorySecrets();
        var vault = new PluginCredentialVault(secrets, new SqlitePluginCredentialMetadataStore(database));
        var scope = Scope("api.token");

        await vault.UpsertAsync(scope, "super-secret-value");

        Assert.Equal("super-secret-value", await vault.ResolveAsync(scope));
        var metadata = Assert.Single(await vault.ListAsync("owner-1", "device-1", "plugin-1", "release-1"));
        Assert.Equal("api.token", metadata.Scope.SecretName);
        await using var connection = await database.OpenConnectionAsync();
        var command = connection.CreateCommand();
        command.CommandText = "SELECT sql FROM sqlite_master WHERE name = 'plugin_credential_metadata';";
        var schema = Assert.IsType<string>(await command.ExecuteScalarAsync());
        Assert.DoesNotContain("secret_value", schema, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public async Task MetadataFailureRestoresPreviousSecret()
    {
        var secrets = new MemorySecrets();
        var metadata = new FailingMetadataStore();
        var vault = new PluginCredentialVault(secrets, metadata);
        var scope = Scope("api.token");
        await vault.UpsertAsync(scope, "old-secret");
        metadata.FailSave = true;

        await Assert.ThrowsAsync<IOException>(() => vault.UpsertAsync(scope, "new-secret"));

        metadata.FailSave = false;
        Assert.Equal("old-secret", await vault.ResolveAsync(scope));
    }

    [Fact]
    public void ScopeHashIsStableAndBindsEveryIdentityField()
    {
        var first = Scope("api.token");
        var same = Scope("api.token");
        var other = Scope("other.token");

        Assert.Equal(first.ScopeHash, same.ScopeHash);
        Assert.NotEqual(first.ScopeHash, other.ScopeHash);
        Assert.Equal(64, first.ScopeHash.Length);
    }

    public void Dispose()
    {
        try
        {
            if (Directory.Exists(_directory))
            {
                Directory.Delete(_directory, recursive: true);
            }
        }
        catch (IOException)
        {
        }
    }

    private static PluginCredentialScope Scope(string name) => new(
        "owner-1",
        "device-1",
        "plugin-1",
        "release-1",
        "main",
        name);

    private sealed class MemorySecrets : IConnectorSecretStore
    {
        private readonly Dictionary<string, string> _values = new(StringComparer.Ordinal);

        public ValueTask<string?> GetAsync(string key, CancellationToken cancellationToken = default) =>
            ValueTask.FromResult(_values.GetValueOrDefault(key));

        public ValueTask SetAsync(string key, string value, CancellationToken cancellationToken = default)
        {
            _values[key] = value;
            return ValueTask.CompletedTask;
        }

        public ValueTask DeleteAsync(string key, CancellationToken cancellationToken = default)
        {
            _values.Remove(key);
            return ValueTask.CompletedTask;
        }
    }

    private sealed class FailingMetadataStore : IPluginCredentialMetadataStore
    {
        private PluginCredentialMetadata? _value;
        public bool FailSave { get; set; }

        public Task<PluginCredentialMetadata?> GetAsync(
            PluginCredentialScope scope,
            CancellationToken cancellationToken) => Task.FromResult(_value);

        public Task SaveAsync(PluginCredentialMetadata metadata, CancellationToken cancellationToken)
        {
            if (FailSave)
            {
                throw new IOException("metadata unavailable");
            }

            _value = metadata;
            return Task.CompletedTask;
        }

        public Task DeleteAsync(PluginCredentialScope scope, CancellationToken cancellationToken)
        {
            _value = null;
            return Task.CompletedTask;
        }

        public Task<IReadOnlyList<PluginCredentialMetadata>> ListAsync(
            string ownerUserId,
            string deviceId,
            string pluginId,
            string? releaseId,
            CancellationToken cancellationToken) =>
            Task.FromResult<IReadOnlyList<PluginCredentialMetadata>>(
                _value is null ? [] : [_value]);
    }
}
