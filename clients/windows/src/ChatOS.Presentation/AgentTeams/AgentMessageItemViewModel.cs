using System.Globalization;
using ChatOS.Core.Domain;

namespace ChatOS.Presentation.AgentTeams;

public sealed class AgentMessageItemViewModel
{
    public AgentMessageItemViewModel(AgentMessage message, string? agentName = null)
    {
        Message = message;
        SenderLabel = message.SenderKind switch
        {
            AgentMessageSenderKind.Human => "你",
            AgentMessageSenderKind.System => "系统",
            _ => string.IsNullOrWhiteSpace(agentName)
                ? message.SenderAgentId ?? "Agent"
                : agentName,
        };
        TimestampLabel = FormatTimestamp(message.CreatedAtUnixMs);
    }

    public AgentMessage Message { get; }
    public string Content => Message.Content;
    public IReadOnlyList<AgentMessageAttachment> Attachments => Message.Attachments;
    public string SenderLabel { get; }
    public string TimestampLabel { get; }
    public bool IsHuman => Message.SenderKind == AgentMessageSenderKind.Human;
    public string SenderGlyph => Message.SenderKind == AgentMessageSenderKind.System
        ? "\uE946"
        : "\uE77B";

    internal static string FormatTimestamp(long unixMilliseconds)
    {
        var timestamp = DateTimeOffset.FromUnixTimeMilliseconds(unixMilliseconds).ToLocalTime();
        var now = DateTimeOffset.Now;
        if (timestamp.Date == now.Date)
        {
            return timestamp.ToString("HH:mm", CultureInfo.CurrentCulture);
        }

        var format = timestamp.Year == now.Year ? "MM-dd HH:mm" : "yyyy-MM-dd HH:mm";
        return timestamp.ToString(format, CultureInfo.CurrentCulture);
    }
}
