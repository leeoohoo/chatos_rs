namespace ChatOS.Connector.AgentTeams;

internal sealed class AgentRunReferenceVault
{
    public AgentRunReferenceVault(bool allowLegacyIds = false)
    {
        AllowsLegacyIds = allowLegacyIds;
    }

    public bool AllowsLegacyIds { get; }
    internal sealed record MessageAuthority(string RoomId, string MessageId);
    internal sealed record AttachmentAuthority(string RoomId, string AttachmentId);
    internal sealed record TodoAuthority(string RoomId, string TodoId, string AgentId);
    internal sealed record AssetAuthority(string RoomId, string AssetId, int Revision);
    internal sealed record SurveyAuthority(string ProjectId, string SurveyId);

    private readonly Dictionary<string, string> _agents = new(StringComparer.Ordinal);
    private readonly Dictionary<string, string> _rooms = new(StringComparer.Ordinal);
    private readonly Dictionary<string, MessageAuthority> _messages = new(StringComparer.Ordinal);
    private readonly Dictionary<string, AttachmentAuthority> _attachments = new(StringComparer.Ordinal);
    private readonly Dictionary<string, TodoAuthority> _todos = new(StringComparer.Ordinal);
    private readonly Dictionary<string, AssetAuthority> _assets = new(StringComparer.Ordinal);
    private readonly Dictionary<string, SurveyAuthority> _surveys = new(StringComparer.Ordinal);

    public string AgentReference(string agentId) => Issue(_agents, "agent", agentId);
    public string ConversationReference(string roomId) => Issue(_rooms, "conversation", roomId);
    public string MessageReference(string roomId, string messageId) =>
        Issue(_messages, "message", new MessageAuthority(roomId, messageId));
    public string AttachmentReference(string roomId, string attachmentId) =>
        Issue(_attachments, "attachment", new AttachmentAuthority(roomId, attachmentId));
    public string TodoReference(string roomId, string todoId, string agentId) =>
        Issue(_todos, "todo", new TodoAuthority(roomId, todoId, agentId));
    public string AssetReference(string roomId, string assetId, int revision) =>
        Issue(_assets, "asset", new AssetAuthority(roomId, assetId, revision));
    public string SurveyReference(string projectId, string surveyId) =>
        Issue(_surveys, "survey", new SurveyAuthority(projectId, surveyId));

    public string? AgentId(string reference) => Resolve(_agents, reference);
    public string? RoomId(string reference) => Resolve(_rooms, reference);
    public MessageAuthority? Message(string reference) => Resolve(_messages, reference);
    public AttachmentAuthority? Attachment(string reference) => Resolve(_attachments, reference);
    public TodoAuthority? Todo(string reference) => Resolve(_todos, reference);
    public AssetAuthority? Asset(string reference) => Resolve(_assets, reference);
    public SurveyAuthority? Survey(string reference) => Resolve(_surveys, reference);

    private string? Resolve(Dictionary<string, string> values, string reference) =>
        values.GetValueOrDefault(reference) ?? (AllowsLegacyIds ? reference : null);

    private T? Resolve<T>(Dictionary<string, T> values, string reference) where T : class =>
        values.GetValueOrDefault(reference);

    private static string Issue<T>(Dictionary<string, T> values, string prefix, T authority)
        where T : notnull
    {
        foreach (var (reference, value) in values)
        {
            if (EqualityComparer<T>.Default.Equals(value, authority)) return reference;
        }
        var created = $"{prefix}_{Guid.NewGuid():N}";
        values.Add(created, authority);
        return created;
    }
}
