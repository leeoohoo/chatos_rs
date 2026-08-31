using System.Security.Cryptography;
using System.Text;

namespace ChatOS.Connector.Plugins;

internal sealed record PluginCredentialBinding(
    PluginCredentialVault Vault,
    string OwnerUserId,
    string DeviceId,
    string PluginId,
    string ReleaseId,
    string ComponentKey,
    IReadOnlySet<string> SecretNames,
    string SnapshotSha256)
{
    public static async Task<PluginCredentialBinding?> PrepareAsync(
        PluginCredentialVault? vault,
        string ownerUserId,
        string deviceId,
        InstalledPluginRecord record,
        string componentKey,
        IEnumerable<string> secretNames,
        CancellationToken cancellationToken)
    {
        var names = secretNames.ToHashSet(StringComparer.Ordinal);
        if (names.Count == 0)
        {
            return null;
        }

        var availableVault = vault
            ?? throw new PluginRuntimeException("Plugin credential template requires Credential Vault.");
        var binding = new PluginCredentialBinding(
            availableVault,
            ownerUserId,
            deviceId,
            record.PluginId,
            record.ReleaseId,
            componentKey,
            names,
            string.Empty);
        return binding with
        {
            SnapshotSha256 = await binding.CurrentSnapshotAsync(cancellationToken).ConfigureAwait(false),
        };
    }

    public async Task VerifyAsync(CancellationToken cancellationToken)
    {
        var current = await CurrentSnapshotAsync(cancellationToken).ConfigureAwait(false);
        if (!CryptographicOperations.FixedTimeEquals(
                Encoding.ASCII.GetBytes(SnapshotSha256),
                Encoding.ASCII.GetBytes(current)))
        {
            throw new PluginRuntimeException("Plugin MCP credential snapshot changed after prepare.");
        }
    }

    public async Task<string> ResolveAsync(string secretName, CancellationToken cancellationToken)
    {
        if (!SecretNames.Contains(secretName))
        {
            throw new PluginRuntimeException("Plugin MCP credential was not published during prepare.");
        }

        await VerifyAsync(cancellationToken).ConfigureAwait(false);
        return await Vault.ResolveAsync(new PluginCredentialScope(
            OwnerUserId,
            DeviceId,
            PluginId,
            ReleaseId,
            ComponentKey,
            secretName), cancellationToken).ConfigureAwait(false);
    }

    private async Task<string> CurrentSnapshotAsync(CancellationToken cancellationToken)
    {
        var metadata = await Vault.ListAsync(
            OwnerUserId,
            DeviceId,
            PluginId,
            ReleaseId,
            cancellationToken).ConfigureAwait(false);
        var byName = metadata
            .Where(value => string.Equals(value.Scope.ComponentKey, ComponentKey, StringComparison.Ordinal))
            .ToDictionary(value => value.Scope.SecretName, StringComparer.Ordinal);
        var payload = new StringBuilder()
            .Append("chatos.plugin.mcp.credentials.v1\n")
            .Append(OwnerUserId).Append('\n')
            .Append(DeviceId).Append('\n')
            .Append(PluginId).Append('\n')
            .Append(ReleaseId).Append('\n')
            .Append(ComponentKey);
        foreach (var name in SecretNames.Order(StringComparer.Ordinal))
        {
            if (!byName.TryGetValue(name, out var value))
            {
                throw new PluginRuntimeException($"Plugin credential is missing: {name}");
            }

            payload.Append('\n').Append(name).Append(':').Append(value.UpdatedAt.ToString("O"));
        }

        return Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(payload.ToString())))
            .ToLowerInvariant();
    }
}

internal sealed record PluginOAuthTokenBinding(
    string ConnectionId,
    string Provider,
    string Resource,
    IReadOnlyList<string> Scopes,
    string SnapshotSha256);
