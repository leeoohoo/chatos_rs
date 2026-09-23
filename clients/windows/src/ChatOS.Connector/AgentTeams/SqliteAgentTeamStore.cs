using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Connector.Persistence;
using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.AgentTeams;

public sealed partial class SqliteAgentTeamStore(LocalStateDatabase database) : IAgentTeamStore
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    private static long Now() => DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();

    private static string NewId() => Guid.NewGuid().ToString("D").ToLowerInvariant();

    private static string Serialize<T>(T value) => JsonSerializer.Serialize(value, JsonOptions);

    private static IReadOnlyList<string> DeserializeStrings(string value) =>
        JsonSerializer.Deserialize<string[]>(value, JsonOptions) ?? [];

    private static TEnum ParseEnum<TEnum>(string value) where TEnum : struct, Enum =>
        Enum.TryParse<TEnum>(value, ignoreCase: true, out var result)
            ? result
            : throw new InvalidDataException($"Invalid persisted {typeof(TEnum).Name} value.");

    private static object DbValue(string? value) => value is null ? DBNull.Value : value;

    private static object DbValue(long? value) => value is null ? DBNull.Value : value.Value;

    private static SqliteCommand Command(
        SqliteConnection connection,
        SqliteTransaction? transaction,
        string sql,
        params object[] values)
    {
        var command = connection.CreateCommand();
        command.Transaction = transaction;
        command.CommandText = sql;
        for (var index = 0; index < values.Length; index++)
        {
            command.Parameters.AddWithValue($"@p{index}", values[index]);
        }

        return command;
    }

    private static AgentTeamException NotFound(string resource) =>
        new(AgentTeamError.NotFound, $"{resource} was not found.");

    private static AgentTeamException Conflict(string message) =>
        new(AgentTeamError.Conflict, message);

    private static async Task RequireAgentAsync(
        SqliteConnection connection,
        SqliteTransaction? transaction,
        string ownerUserId,
        string agentId,
        bool requireActive,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction,
            "SELECT status FROM agent_profiles WHERE owner_user_id = @p0 AND id = @p1",
            ownerUserId, agentId);
        var value = await command.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false);
        if (value is not string status || requireActive && !status.Equals("Active", StringComparison.OrdinalIgnoreCase))
        {
            throw NotFound("Agent");
        }
    }

    private static async Task RequireRoomAsync(
        SqliteConnection connection,
        SqliteTransaction? transaction,
        string ownerUserId,
        string roomId,
        bool requireActive,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction,
            "SELECT status FROM agent_rooms WHERE owner_user_id = @p0 AND id = @p1",
            ownerUserId, roomId);
        var value = await command.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false);
        if (value is not string status || requireActive && !status.Equals("Active", StringComparison.OrdinalIgnoreCase))
        {
            throw NotFound("Agent room");
        }
    }

    private static async Task RequireActiveMemberAsync(
        SqliteConnection connection,
        SqliteTransaction? transaction,
        string ownerUserId,
        string roomId,
        string agentId,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction, """
            SELECT 1 FROM agent_room_members
            WHERE owner_user_id = @p0 AND room_id = @p1 AND agent_id = @p2 AND status = 'Active'
            """, ownerUserId, roomId, agentId);
        if (await command.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false) is null)
        {
            throw new AgentTeamException(AgentTeamError.NotMember, "Agent is not an active member of this room.");
        }
    }

    private static AgentProfile ReadProfile(SqliteDataReader reader)
    {
        var profile = new AgentProfile(
            reader.GetString(1),
            reader.GetString(0),
            new AgentProfileDraft(
                reader.GetString(2),
                reader.GetString(3),
                reader.GetString(4),
                reader.GetString(5),
                reader.IsDBNull(6) ? null : reader.GetString(6),
                reader.GetString(7),
                DeserializeStrings(reader.GetString(8)),
                DeserializeStrings(reader.GetString(9)),
                reader.GetInt64(10) != 0,
                reader.GetInt32(11),
                reader.GetString(12)),
            ParseEnum<AgentProfileStatus>(reader.GetString(13)),
            reader.GetInt64(14),
            reader.GetInt64(15),
            reader.IsDBNull(16) ? null : reader.GetInt64(16),
            reader.IsDBNull(17) ? null : reader.GetInt64(17));
        profile.Validate();
        return profile;
    }

    private static AgentRoom ReadRoom(SqliteDataReader reader)
    {
        var room = new AgentRoom(
            reader.GetString(1),
            reader.GetString(0),
            reader.GetString(2),
            new AgentRoomDraft(reader.GetString(3), reader.GetString(4)),
            reader.IsDBNull(5) ? null : reader.GetString(5),
            reader.IsDBNull(6) ? null : reader.GetString(6),
            ParseEnum<AgentConversationKind>(reader.GetString(7)),
            reader.IsDBNull(8) ? null : reader.GetString(8),
            ParseEnum<AgentRoomStatus>(reader.GetString(9)),
            reader.GetInt64(10),
            reader.GetInt64(11));
        room.Validate();
        return room;
    }

    private static AgentRoomMember ReadMember(SqliteDataReader reader)
    {
        var member = new AgentRoomMember(
            reader.GetString(0),
            reader.GetString(1),
            reader.GetString(2),
            new AgentRoomMemberDraft(
                reader.GetString(3),
                reader.GetString(4),
                DeserializeStrings(reader.GetString(5))),
            ParseEnum<AgentMemberStatus>(reader.GetString(6)),
            reader.GetInt64(7));
        member.Validate();
        return member;
    }
}
