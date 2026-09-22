namespace ChatOS.Core.Domain;

public enum AgentConversationKind
{
    ProjectTeam,
    HumanAgentDirect,
    AgentAgentDirect,
}

public enum AgentRoomStatus
{
    Active,
    Archived,
}

public sealed record AgentRoomDraft(string Name, string Goal = "")
{
    public void Validate()
    {
        AgentTeamValidation.Text(Name, nameof(Name), 160);
        AgentTeamValidation.OptionalText(Goal, nameof(Goal), 8_000);
    }
}

public sealed record AgentRoom(
    string Id,
    string OwnerUserId,
    string ProjectId,
    AgentRoomDraft Draft,
    string? DefaultAgentId,
    string? ProjectManagerAgentId,
    AgentConversationKind Kind,
    string? DirectKey,
    AgentRoomStatus Status,
    long CreatedAtUnixMs,
    long UpdatedAtUnixMs)
{
    public bool IsDirect => Kind != AgentConversationKind.ProjectTeam;

    public void Validate()
    {
        AgentTeamValidation.Identifier(Id, nameof(Id));
        AgentTeamValidation.Identifier(OwnerUserId, nameof(OwnerUserId));
        AgentTeamValidation.Identifier(ProjectId, nameof(ProjectId));
        Draft.Validate();
        if (DefaultAgentId is not null)
        {
            AgentTeamValidation.Identifier(DefaultAgentId, nameof(DefaultAgentId));
        }

        if (ProjectManagerAgentId is not null)
        {
            AgentTeamValidation.Identifier(ProjectManagerAgentId, nameof(ProjectManagerAgentId));
            if (Kind != AgentConversationKind.ProjectTeam)
            {
                throw AgentTeamValidation.Invalid(nameof(ProjectManagerAgentId));
            }
        }

        if (Kind == AgentConversationKind.ProjectTeam && DirectKey is not null ||
            Kind != AgentConversationKind.ProjectTeam && DirectKey is null)
        {
            throw AgentTeamValidation.Invalid(nameof(DirectKey));
        }

        AgentTeamValidation.Timestamps(CreatedAtUnixMs, UpdatedAtUnixMs);
    }
}

public enum AgentMemberStatus
{
    Active,
    Removed,
}

public sealed record AgentRoomMemberDraft(
    string Role,
    string Responsibility = "",
    IReadOnlyList<string>? PluginAllowlist = null)
{
    public IReadOnlyList<string> Plugins => PluginAllowlist ?? [];

    public void Validate()
    {
        AgentTeamValidation.Text(Role, nameof(Role), 160);
        AgentTeamValidation.OptionalText(Responsibility, nameof(Responsibility), 8_000);
        AgentTeamValidation.Identifiers(Plugins, nameof(PluginAllowlist), 100);
    }
}

public sealed record AgentRoomMember(
    string OwnerUserId,
    string RoomId,
    string AgentId,
    AgentRoomMemberDraft Draft,
    AgentMemberStatus Status,
    long JoinedAtUnixMs)
{
    public string Id => $"{RoomId}:{AgentId}";

    public void Validate()
    {
        AgentTeamValidation.Identifier(OwnerUserId, nameof(OwnerUserId));
        AgentTeamValidation.Identifier(RoomId, nameof(RoomId));
        AgentTeamValidation.Identifier(AgentId, nameof(AgentId));
        Draft.Validate();
        if (JoinedAtUnixMs < 0)
        {
            throw AgentTeamValidation.Invalid(nameof(JoinedAtUnixMs));
        }
    }
}

public sealed record AgentUnreadConversation(
    AgentRoom Room,
    IReadOnlyList<AgentMessage> Messages);

public enum AgentMessageSenderKind
{
    Human,
    Agent,
    System,
}

public enum AgentMessageAttachmentKind
{
    File,
    Image,
    Audio,
}

public sealed record AgentMessageAttachment(
    string Id,
    string Name,
    string MimeType,
    AgentMessageAttachmentKind Kind,
    long ByteCount,
    byte[] Data)
{
    public bool HasPayload => Data.LongLength == ByteCount;

    public void Validate(bool requirePayload = true)
    {
        AgentTeamValidation.Identifier(Id, nameof(Id));
        AgentTeamValidation.Text(Name, nameof(Name), 512);
        AgentTeamValidation.Text(MimeType, nameof(MimeType), 160);
        if (ByteCount <= 0 || ByteCount > 20L * 1024 * 1024 ||
            requirePayload && !HasPayload || !requirePayload && Data.LongLength is not 0 && !HasPayload)
        {
            throw AgentTeamValidation.Invalid(nameof(Data));
        }
    }
}

public sealed record AgentMessageDraft(
    AgentMessageSenderKind SenderKind,
    string? SenderAgentId,
    string Content,
    IReadOnlyList<string>? MentionedAgentIds = null,
    IReadOnlyList<AgentMessageAttachment>? Attachments = null,
    string? ReplyToMessageId = null,
    string? RootMessageId = null,
    int HopCount = 0)
{
    public IReadOnlyList<string> Mentions => MentionedAgentIds ?? [];

    public IReadOnlyList<AgentMessageAttachment> AttachmentItems => Attachments ?? [];

    public void Validate()
    {
        if (SenderKind == AgentMessageSenderKind.Agent)
        {
            AgentTeamValidation.Identifier(SenderAgentId, nameof(SenderAgentId));
        }
        else if (SenderAgentId is not null)
        {
            throw AgentTeamValidation.Invalid(nameof(SenderAgentId));
        }

        if (string.IsNullOrWhiteSpace(Content) && AttachmentItems.Count == 0)
        {
            throw AgentTeamValidation.Invalid(nameof(Content));
        }

        AgentTeamValidation.OptionalText(Content, nameof(Content), 64_000);
        AgentTeamValidation.Identifiers(Mentions, nameof(MentionedAgentIds), 64);
        if (AttachmentItems.Count > 20 || AttachmentItems.Sum(value => value.ByteCount) > 20L * 1024 * 1024 ||
            HopCount is < 0 or > 32)
        {
            throw AgentTeamValidation.Invalid("message routing");
        }

        foreach (var attachment in AttachmentItems)
        {
            attachment.Validate();
        }
    }
}

public sealed record AgentMessage(
    string Id,
    string OwnerUserId,
    string RoomId,
    AgentMessageSenderKind SenderKind,
    string? SenderAgentId,
    string Content,
    IReadOnlyList<string> MentionedAgentIds,
    IReadOnlyList<AgentMessageAttachment> Attachments,
    string? ReplyToMessageId,
    string RootMessageId,
    int HopCount,
    long CreatedAtUnixMs)
{
    public void Validate()
    {
        AgentTeamValidation.Identifier(Id, nameof(Id));
        AgentTeamValidation.Identifier(OwnerUserId, nameof(OwnerUserId));
        AgentTeamValidation.Identifier(RoomId, nameof(RoomId));
        if (SenderKind == AgentMessageSenderKind.Agent)
            AgentTeamValidation.Identifier(SenderAgentId, nameof(SenderAgentId));
        else if (SenderAgentId is not null)
            throw AgentTeamValidation.Invalid(nameof(SenderAgentId));
        if (string.IsNullOrWhiteSpace(Content) && Attachments.Count == 0)
            throw AgentTeamValidation.Invalid(nameof(Content));
        AgentTeamValidation.OptionalText(Content, nameof(Content), 64_000);
        AgentTeamValidation.Identifiers(MentionedAgentIds, nameof(MentionedAgentIds), 64);
        if (Attachments.Count > 20 || Attachments.Sum(value => value.ByteCount) > 20L * 1024 * 1024 ||
            HopCount is < 0 or > 32)
            throw AgentTeamValidation.Invalid("message routing");
        foreach (var attachment in Attachments) attachment.Validate(requirePayload: false);
        AgentTeamValidation.Identifier(RootMessageId, nameof(RootMessageId));
        if (CreatedAtUnixMs < 0)
        {
            throw AgentTeamValidation.Invalid(nameof(CreatedAtUnixMs));
        }
    }
}

public sealed record AgentPostResult(
    AgentMessage Message,
    IReadOnlyList<AgentDelivery> Deliveries,
    string? RoutingStopReason = null);
