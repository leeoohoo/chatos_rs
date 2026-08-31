using ChatOS.Connector.Gateway;
using ChatOS.Connector.Runtime;

namespace ChatOS.Connector.Plugins;

public sealed record LocalConnectorPlugin(
    string PluginId,
    string DisplayName,
    string Description,
    string Category,
    string Publisher,
    string LatestVersion,
    bool Installed,
    bool UpdateAvailable,
    bool InstallAvailable,
    bool Enabled,
    IReadOnlyList<string> DeclaredPermissions);

public interface ILocalPluginManagementService
{
    Task<IReadOnlyList<LocalConnectorPlugin>> ListAsync(
        CancellationToken cancellationToken = default);

    Task<InstalledPluginRecord> InstallAsync(
        string pluginId,
        CancellationToken cancellationToken = default);

    Task UninstallAsync(
        string pluginId,
        CancellationToken cancellationToken = default);

    Task SetEnabledAsync(
        string pluginId,
        bool enabled,
        CancellationToken cancellationToken = default);
}

internal sealed class LocalPluginManagementService(
    ConnectorRuntimeContext runtime,
    IConnectorGatewayClient gateway,
    WindowsPluginPackageInstaller installer,
    IInstalledPluginStore store,
    PluginCredentialVault credentials,
    PluginOAuthBroker oauth,
    PluginRuntimeSessionStore sessions) : ILocalPluginManagementService
{
    public async Task<IReadOnlyList<LocalConnectorPlugin>> ListAsync(
        CancellationToken cancellationToken = default)
    {
        var session = await RequireSessionAsync(cancellationToken).ConfigureAwait(false);
        var sources = await gateway.ListPluginSourcesAsync(
            session.GatewayBaseUri,
            session.AccessToken,
            cancellationToken).ConfigureAwait(false);
        var installed = (await store.ListAsync(cancellationToken).ConfigureAwait(false))
            .ToDictionary(record => record.PluginId, StringComparer.Ordinal);
        return sources.Select(source =>
        {
            installed.TryGetValue(source.Catalog.Id, out var record);
            var version = source.Release.Version ?? source.Release.Id;
            return new LocalConnectorPlugin(
                source.Catalog.Id,
                source.Catalog.DisplayName ?? source.Catalog.Name ?? source.Catalog.Id,
                source.Catalog.Description ?? string.Empty,
                source.Catalog.Category ?? "Plugin",
                source.Catalog.PublisherName ?? source.Catalog.DeveloperName ?? "ChatOS",
                version,
                record is not null,
                record is not null && !string.Equals(record.Version, version, StringComparison.Ordinal),
                source.Release.NpmPackage is not null && IsSha256(source.Release.ArtifactSha256),
                source.Preference?.Enabled ?? true,
                record?.DeclaredPermissions ?? Array.Empty<string>());
        }).ToArray();
    }

    public async Task<InstalledPluginRecord> InstallAsync(
        string pluginId,
        CancellationToken cancellationToken = default)
    {
        var session = await RequireSessionAsync(cancellationToken).ConfigureAwait(false);
        var sources = await gateway.ListPluginSourcesAsync(
            session.GatewayBaseUri,
            session.AccessToken,
            cancellationToken).ConfigureAwait(false);
        var source = sources.FirstOrDefault(item =>
            string.Equals(item.Catalog.Id, pluginId, StringComparison.Ordinal))
            ?? throw new KeyNotFoundException("Plugin is not available from the configured marketplace.");

        var previous = await store.GetAsync(pluginId, cancellationToken).ConfigureAwait(false);
        var temporaryPath = Path.Combine(Path.GetTempPath(), $"chatos-plugin-{Guid.NewGuid():N}.tgz");
        try
        {
            await using (var destination = new FileStream(
                temporaryPath,
                FileMode.CreateNew,
                FileAccess.Write,
                FileShare.None,
                64 * 1024,
                FileOptions.Asynchronous | FileOptions.SequentialScan))
            {
                await gateway.DownloadPluginArtifactAsync(
                    session.GatewayBaseUri,
                    session.AccessToken,
                    source.Catalog.Id,
                    source.Release.Id,
                    destination,
                    cancellationToken).ConfigureAwait(false);
                await destination.FlushAsync(cancellationToken).ConfigureAwait(false);
            }

            var record = await installer.InstallAsync(source, temporaryPath, cancellationToken)
                .ConfigureAwait(false);
            try
            {
                await store.SaveAsync(record, cancellationToken).ConfigureAwait(false);
            }
            catch
            {
                await installer.UninstallAsync(pluginId, CancellationToken.None).ConfigureAwait(false);
                throw;
            }

            if (previous is not null &&
                !string.Equals(previous.ReleaseId, record.ReleaseId, StringComparison.Ordinal) &&
                runtime.Snapshot.State is { } state)
            {
                await sessions.TerminatePluginAsync(pluginId).ConfigureAwait(false);
                await oauth.PurgePluginAsync(
                    state.User.Id,
                    state.DeviceId,
                    pluginId,
                    cancellationToken).ConfigureAwait(false);
                await credentials.PurgePluginAsync(
                    state.User.Id,
                    state.DeviceId,
                    pluginId,
                    cancellationToken).ConfigureAwait(false);
            }

            return record;
        }
        finally
        {
            try
            {
                File.Delete(temporaryPath);
            }
            catch (IOException)
            {
            }
            catch (UnauthorizedAccessException)
            {
            }
        }
    }

    public async Task UninstallAsync(
        string pluginId,
        CancellationToken cancellationToken = default)
    {
        var state = runtime.Snapshot.State;
        await sessions.TerminatePluginAsync(pluginId).ConfigureAwait(false);
        if (state is not null)
        {
            await oauth.PurgePluginAsync(
                state.User.Id,
                state.DeviceId,
                pluginId,
                cancellationToken).ConfigureAwait(false);
        }

        await installer.UninstallAsync(pluginId, cancellationToken).ConfigureAwait(false);
        await store.DeleteAsync(pluginId, cancellationToken).ConfigureAwait(false);
        if (state is not null)
        {
            await credentials.PurgePluginAsync(
                state.User.Id,
                state.DeviceId,
                pluginId,
                cancellationToken).ConfigureAwait(false);
        }
    }

    public async Task SetEnabledAsync(
        string pluginId,
        bool enabled,
        CancellationToken cancellationToken = default)
    {
        var session = await RequireSessionAsync(cancellationToken).ConfigureAwait(false);
        await gateway.UpdatePluginPreferenceAsync(
            session.GatewayBaseUri,
            session.AccessToken,
            pluginId,
            session.DeviceId,
            enabled,
            cancellationToken).ConfigureAwait(false);
        if (!enabled)
        {
            await sessions.TerminatePluginAsync(pluginId).ConfigureAwait(false);
        }
    }

    private async Task<ConnectorSessionConfiguration> RequireSessionAsync(
        CancellationToken cancellationToken)
    {
        await runtime.InitializeAsync(cancellationToken).ConfigureAwait(false);
        return await runtime.SessionConfigurationAsync(cancellationToken).ConfigureAwait(false)
            ?? throw new InvalidOperationException("Local Connector is not paired.");
    }

    private static bool IsSha256(string? value) =>
        value is { Length: 64 } && value.All(Uri.IsHexDigit);
}
