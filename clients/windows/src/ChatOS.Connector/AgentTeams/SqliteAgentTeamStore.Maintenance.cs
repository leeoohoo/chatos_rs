using ChatOS.Core.Domain;
using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.AgentTeams;

public sealed partial class SqliteAgentTeamStore
{
    private static async Task EnqueueTeamAssetMaintenanceAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        AgentRoom room,
        string projectManagerAgentId,
        long now,
        CancellationToken cancellationToken)
    {
        var deduplicationKey = $"team-asset-maintenance:{room.Id}:v1";
        var messageId = $"asset-maintenance-message-{room.Id}";
        var deliveryId = $"asset-maintenance-delivery-{room.Id}";
        var content = $"""
            你已被明确指定为“{room.Draft.Name}”的项目经理。请读取 Human 消息、团队目标、成员和 Todo 状态，主动维护真实的团队共享资产。信息充分时建立或更新“项目概览”和“当前进度”；信息不足时先用 requirement_survey_create 发起需求调研，不要写空模板或臆测内容。完成本轮实际处理后再结束通讯周期。
            """;

        using (var message = Command(connection, transaction, """
            INSERT OR IGNORE INTO agent_messages (
                owner_user_id, id, room_id, sender_kind, sender_agent_id, content,
                reply_to_message_id, root_message_id, hop_count, created_at_unix_ms)
            VALUES (@p0, @p1, @p2, 'System', NULL, @p3, NULL, @p1, 0, @p4)
            """, room.OwnerUserId, messageId, room.Id, content, now))
        {
            await message.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        }

        using (var mention = Command(connection, transaction, """
            INSERT OR IGNORE INTO agent_message_mentions (
                owner_user_id, message_id, agent_id)
            VALUES (@p0, @p1, @p2)
            """, room.OwnerUserId, messageId, projectManagerAgentId))
        {
            await mention.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        }

        using var delivery = Command(connection, transaction, """
            INSERT OR IGNORE INTO agent_deliveries (
                owner_user_id, id, room_id, message_id, root_message_id, target_agent_id,
                trigger_kind, status, attempt, hop_count, deduplication_key,
                response_message_id, last_error, claimed_at_unix_ms,
                completed_at_unix_ms, created_at_unix_ms)
            VALUES (@p0, @p1, @p2, @p3, @p3, @p4, 'Mention', 'Pending', 0, 0,
                @p5, NULL, NULL, NULL, NULL, @p6)
            """, room.OwnerUserId, deliveryId, room.Id, messageId,
            projectManagerAgentId, deduplicationKey, now);
        await delivery.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }
}
