using System.Security.Cryptography;
using System.Text;
using ChatOS.Connector.Persistence;
using ChatOS.Connector.Security;

namespace ChatOS.Connector.Plugins;

public sealed record PluginCredentialScope
{
    public PluginCredentialScope(
        string ownerUserId,
        string deviceId,
        string pluginId,
        string releaseId,
        string componentKey,
        string secretName)
    {
        OwnerUserId = Validate(ownerUserId, 256, allowPunctuation: true, nameof(ownerUserId));
        DeviceId = Validate(deviceId, 256, allowPunctuation: true, nameof(deviceId));
        PluginId = Validate(pluginId, 256, allowPunctuation: true, nameof(pluginId));
        ReleaseId = Validate(releaseId, 256, allowPunctuation: true, nameof(releaseId));
        ComponentKey = Validate(componentKey, 128, allowPunctuation: false, nameof(componentKey));
        SecretName = Validate(secretName, 128, allowPunctuation: false, nameof(secretName));
    }

    public string OwnerUserId { get; }
    public string DeviceId { get; }
    public string PluginId { get; }
    public string ReleaseId { get; }
    public string ComponentKey { get; }
    public string SecretName { get; }

    public string ScopeHash
    {
        get
        {
            using var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
            hash.AppendData("chatos-plugin-credential-scope-v1\0"u8);
            foreach (var value in new[]
                     {
                         OwnerUserId, DeviceId, PluginId, ReleaseId, ComponentKey, SecretName,
                     })
            {
                var bytes = Encoding.UTF8.GetBytes(value);
                hash.AppendData(LengthBytes(bytes.Length));
                hash.AppendData(bytes);
            }

            return Convert.ToHexString(hash.GetHashAndReset()).ToLowerInvariant();
        }
    }

    private static byte[] LengthBytes(int length)
    {
        var bytes = new byte[8];
        System.Buffers.Binary.BinaryPrimitives.WriteUInt64BigEndian(bytes, checked((ulong)length));
        return bytes;
    }

    private static string Validate(string value, int maximum, bool allowPunctuation, string parameter)
    {
        var trimmed = value?.Trim();
        var invalidCharacters = !allowPunctuation && trimmed is not null && trimmed.Any(character =>
            !(char.IsAsciiLetterOrDigit(character) || character is '.' or '-' or '_' or ':'));
        if (string.IsNullOrEmpty(trimmed) ||
            Encoding.UTF8.GetByteCount(trimmed) > maximum ||
            trimmed.Any(char.IsControl) ||
            invalidCharacters)
        {
            throw new ArgumentException("Plugin credential scope value is invalid.", parameter);
        }

        return trimmed;
    }
}

public sealed record PluginCredentialMetadata(
    PluginCredentialScope Scope,
    DateTimeOffset CreatedAt,
    DateTimeOffset UpdatedAt);

public interface IPluginCredentialMetadataStore
{
    Task<PluginCredentialMetadata?> GetAsync(PluginCredentialScope scope, CancellationToken cancellationToken);
    Task SaveAsync(PluginCredentialMetadata metadata, CancellationToken cancellationToken);
    Task DeleteAsync(PluginCredentialScope scope, CancellationToken cancellationToken);
    Task<IReadOnlyList<PluginCredentialMetadata>> ListAsync(
        string ownerUserId,
        string deviceId,
        string pluginId,
        string? releaseId,
        CancellationToken cancellationToken);
}

internal sealed class SqlitePluginCredentialMetadataStore(LocalStateDatabase database) :
    IPluginCredentialMetadataStore
{
    public async Task<PluginCredentialMetadata?> GetAsync(
        PluginCredentialScope scope,
        CancellationToken cancellationToken)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            SELECT created_at, updated_at
            FROM plugin_credential_metadata
            WHERE scope_hash = $scope_hash
            LIMIT 1;
            """;
        command.Parameters.AddWithValue("$scope_hash", scope.ScopeHash);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        return await reader.ReadAsync(cancellationToken).ConfigureAwait(false)
            ? new PluginCredentialMetadata(
                scope,
                DateTimeOffset.Parse(reader.GetString(0), System.Globalization.CultureInfo.InvariantCulture),
                DateTimeOffset.Parse(reader.GetString(1), System.Globalization.CultureInfo.InvariantCulture))
            : null;
    }

    public async Task SaveAsync(PluginCredentialMetadata metadata, CancellationToken cancellationToken)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            INSERT INTO plugin_credential_metadata(
                scope_hash, owner_user_id, device_id, plugin_id, release_id,
                component_key, secret_name, created_at, updated_at)
            VALUES (
                $scope_hash, $owner_user_id, $device_id, $plugin_id, $release_id,
                $component_key, $secret_name, $created_at, $updated_at)
            ON CONFLICT(scope_hash) DO UPDATE SET
                updated_at = excluded.updated_at;
            """;
        var scope = metadata.Scope;
        command.Parameters.AddWithValue("$scope_hash", scope.ScopeHash);
        command.Parameters.AddWithValue("$owner_user_id", scope.OwnerUserId);
        command.Parameters.AddWithValue("$device_id", scope.DeviceId);
        command.Parameters.AddWithValue("$plugin_id", scope.PluginId);
        command.Parameters.AddWithValue("$release_id", scope.ReleaseId);
        command.Parameters.AddWithValue("$component_key", scope.ComponentKey);
        command.Parameters.AddWithValue("$secret_name", scope.SecretName);
        command.Parameters.AddWithValue("$created_at", metadata.CreatedAt.ToString("O"));
        command.Parameters.AddWithValue("$updated_at", metadata.UpdatedAt.ToString("O"));
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }

    public async Task DeleteAsync(PluginCredentialScope scope, CancellationToken cancellationToken)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = "DELETE FROM plugin_credential_metadata WHERE scope_hash = $scope_hash;";
        command.Parameters.AddWithValue("$scope_hash", scope.ScopeHash);
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }

    public async Task<IReadOnlyList<PluginCredentialMetadata>> ListAsync(
        string ownerUserId,
        string deviceId,
        string pluginId,
        string? releaseId,
        CancellationToken cancellationToken)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            SELECT release_id, component_key, secret_name, created_at, updated_at
            FROM plugin_credential_metadata
            WHERE owner_user_id = $owner_user_id
              AND device_id = $device_id
              AND plugin_id = $plugin_id
              AND ($release_id IS NULL OR release_id = $release_id)
            ORDER BY component_key, secret_name;
            """;
        command.Parameters.AddWithValue("$owner_user_id", ownerUserId);
        command.Parameters.AddWithValue("$device_id", deviceId);
        command.Parameters.AddWithValue("$plugin_id", pluginId);
        command.Parameters.AddWithValue("$release_id", (object?)releaseId ?? DBNull.Value);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        var result = new List<PluginCredentialMetadata>();
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
        {
            var scope = new PluginCredentialScope(
                ownerUserId,
                deviceId,
                pluginId,
                reader.GetString(0),
                reader.GetString(1),
                reader.GetString(2));
            result.Add(new PluginCredentialMetadata(
                scope,
                DateTimeOffset.Parse(reader.GetString(3), System.Globalization.CultureInfo.InvariantCulture),
                DateTimeOffset.Parse(reader.GetString(4), System.Globalization.CultureInfo.InvariantCulture)));
        }

        return result;
    }
}

public sealed class PluginCredentialVault(
    IConnectorSecretStore secrets,
    IPluginCredentialMetadataStore metadata)
{
    internal const int MaximumSecretBytes = 64 * 1024;

    public async Task UpsertAsync(
        PluginCredentialScope scope,
        string value,
        CancellationToken cancellationToken = default)
    {
        if (Encoding.UTF8.GetByteCount(value) > MaximumSecretBytes || value.Contains('\0'))
        {
            throw new ArgumentException("Plugin credential is too large or contains NUL.", nameof(value));
        }

        var previousValue = await secrets.GetAsync(Key(scope), cancellationToken).ConfigureAwait(false);
        var previousMetadata = await metadata.GetAsync(scope, cancellationToken).ConfigureAwait(false);
        var now = DateTimeOffset.UtcNow;
        await secrets.SetAsync(Key(scope), value, cancellationToken).ConfigureAwait(false);
        try
        {
            await metadata.SaveAsync(
                new PluginCredentialMetadata(scope, previousMetadata?.CreatedAt ?? now, now),
                cancellationToken).ConfigureAwait(false);
        }
        catch
        {
            if (previousValue is null)
            {
                await secrets.DeleteAsync(Key(scope), CancellationToken.None).ConfigureAwait(false);
            }
            else
            {
                await secrets.SetAsync(Key(scope), previousValue, CancellationToken.None).ConfigureAwait(false);
            }

            throw;
        }
    }

    public async Task<string> ResolveAsync(
        PluginCredentialScope scope,
        CancellationToken cancellationToken = default)
    {
        if (await metadata.GetAsync(scope, cancellationToken).ConfigureAwait(false) is null)
        {
            throw new PluginRuntimeException($"Plugin credential is missing: {scope.SecretName}");
        }

        return await secrets.GetAsync(Key(scope), cancellationToken).ConfigureAwait(false)
            ?? throw new PluginRuntimeException($"Plugin credential is missing: {scope.SecretName}");
    }

    public Task<IReadOnlyList<PluginCredentialMetadata>> ListAsync(
        string ownerUserId,
        string deviceId,
        string pluginId,
        string? releaseId = null,
        CancellationToken cancellationToken = default) =>
        metadata.ListAsync(ownerUserId, deviceId, pluginId, releaseId, cancellationToken);

    public async Task DeleteAsync(
        PluginCredentialScope scope,
        CancellationToken cancellationToken = default)
    {
        await secrets.DeleteAsync(Key(scope), cancellationToken).ConfigureAwait(false);
        await metadata.DeleteAsync(scope, cancellationToken).ConfigureAwait(false);
    }

    public async Task PurgePluginAsync(
        string ownerUserId,
        string deviceId,
        string pluginId,
        CancellationToken cancellationToken = default)
    {
        var records = await ListAsync(ownerUserId, deviceId, pluginId, cancellationToken: cancellationToken)
            .ConfigureAwait(false);
        foreach (var record in records)
        {
            await DeleteAsync(record.Scope, cancellationToken).ConfigureAwait(false);
        }
    }

    private static string Key(PluginCredentialScope scope) => $"plugin-credential:{scope.ScopeHash}";
}
