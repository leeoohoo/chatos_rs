namespace ChatOS.Core.Domain;

public sealed record LocalAgentContactSkill(string Id, string Name, string Content);

public sealed record LocalAgentContactRuntimeContext(
    string AgentId,
    string Name,
    string? Description,
    string? Category,
    string RoleDefinition,
    IReadOnlyList<LocalAgentContactSkill> Skills,
    string Revision);

public sealed record LocalAgentConversationScope(
    string AccountId,
    string ThreadId,
    string? ProjectId,
    string? ContactAgentId);

public sealed record LocalAgentMainChatTurn(
    LocalAgentRunSnapshot Run,
    LocalAgentMainChatRunBinding Binding,
    LocalAgentRunDetail Detail,
    IReadOnlyList<LocalAgentTaskSnapshot> Tasks);

public sealed record LocalAgentConversationSnapshot(
    string AccountId,
    string ThreadId,
    IReadOnlyList<LocalAgentMainChatTurn> Turns);

public sealed record LocalAgentCreateConversationTurn(
    LocalAgentConversationScope Scope,
    string TurnId,
    string MessageId,
    string? Content,
    IReadOnlyList<ConversationAttachmentDraft> Attachments);
