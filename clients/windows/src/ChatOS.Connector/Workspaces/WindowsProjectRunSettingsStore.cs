using System.Text.Json;
using ChatOS.Connector.Persistence;

namespace ChatOS.Connector.Workspaces;

internal sealed record WindowsProjectRunSettings(
    string? DefaultTargetId,
    IReadOnlyDictionary<string, string> SelectedToolchains,
    IReadOnlyDictionary<string, ChatOS.Core.Domain.ProjectRunCustomToolchain> CustomToolchains,
    IReadOnlyDictionary<string, string> EnvironmentVariables)
{
    public static WindowsProjectRunSettings Empty { get; } = new(
        null,
        new Dictionary<string, string>(),
        new Dictionary<string, ChatOS.Core.Domain.ProjectRunCustomToolchain>(),
        new Dictionary<string, string>());
}

public sealed class WindowsProjectRunSettingsStore(LocalStateDatabase database)
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    internal async Task<WindowsProjectRunSettings> LoadAsync(
        string ownerUserId,
        string projectId,
        CancellationToken cancellationToken)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = "SELECT value FROM ui_state WHERE key = $key LIMIT 1;";
        command.Parameters.AddWithValue("$key", Key(ownerUserId, projectId));
        var value = await command.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false);
        if (value is not string json || string.IsNullOrWhiteSpace(json))
        {
            return WindowsProjectRunSettings.Empty;
        }
        try
        {
            return JsonSerializer.Deserialize<WindowsProjectRunSettings>(json, JsonOptions)
                ?? WindowsProjectRunSettings.Empty;
        }
        catch (JsonException)
        {
            return WindowsProjectRunSettings.Empty;
        }
    }

    internal async Task SaveAsync(
        string ownerUserId,
        string projectId,
        WindowsProjectRunSettings settings,
        CancellationToken cancellationToken)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            INSERT INTO ui_state(key, value, updated_at)
            VALUES ($key, $value, $updatedAt)
            ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at;
            """;
        command.Parameters.AddWithValue("$key", Key(ownerUserId, projectId));
        command.Parameters.AddWithValue("$value", JsonSerializer.Serialize(settings, JsonOptions));
        command.Parameters.AddWithValue("$updatedAt", DateTimeOffset.UtcNow.ToString("O"));
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }

    private static string Key(string ownerUserId, string projectId) =>
        $"project_run_v1:{ownerUserId}:{projectId}";
}
