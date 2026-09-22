using System.Security.Cryptography;
using System.Text;
using ChatOS.Core.Domain;

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
    internal sealed record PluginAuthority(string PluginId, string DisplayName);
    internal sealed record DocumentDraft(
        string Reference,
        AgentMessageAttachment Attachment,
        string Title,
        string Sha256,
        bool Consumed);
    private sealed record SendReceipt(string Signature, AgentToolExecutionResult Result);

    private readonly Dictionary<string, string> _agents = new(StringComparer.Ordinal);
    private readonly Dictionary<string, string> _rooms = new(StringComparer.Ordinal);
    private readonly Dictionary<string, MessageAuthority> _messages = new(StringComparer.Ordinal);
    private readonly Dictionary<string, AttachmentAuthority> _attachments = new(StringComparer.Ordinal);
    private readonly Dictionary<string, TodoAuthority> _todos = new(StringComparer.Ordinal);
    private readonly Dictionary<string, AssetAuthority> _assets = new(StringComparer.Ordinal);
    private readonly Dictionary<string, SurveyAuthority> _surveys = new(StringComparer.Ordinal);
    private readonly Dictionary<string, PluginAuthority> _plugins = new(StringComparer.Ordinal);
    private readonly Dictionary<string, DocumentDraft> _documents = new(StringComparer.Ordinal);
    private readonly Dictionary<string, SendReceipt> _sendReceipts = new(StringComparer.Ordinal);
    private long _documentBytes;

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
    public string PluginReference(string pluginId, string displayName) =>
        Issue(_plugins, "plugin", new PluginAuthority(pluginId, displayName));

    public string? AgentId(string reference) => Resolve(_agents, reference);
    public string? RoomId(string reference) => Resolve(_rooms, reference);
    public MessageAuthority? Message(string reference) => Resolve(_messages, reference);
    public AttachmentAuthority? Attachment(string reference) => Resolve(_attachments, reference);
    public TodoAuthority? Todo(string reference) => Resolve(_todos, reference);
    public AssetAuthority? Asset(string reference) => Resolve(_assets, reference);
    public SurveyAuthority? Survey(string reference) => Resolve(_surveys, reference);
    public PluginAuthority? Plugin(string reference) => Resolve(_plugins, reference);

    public DocumentDraft CreateDocument(string name, string title, string markdown)
    {
        AgentTeamValidation.Text(title, nameof(title), 512);
        AgentTeamValidation.Text(markdown, nameof(markdown), 2 * 1024 * 1024);
        if (_documents.Count >= 8) throw AgentTeamValidation.Invalid("document count");
        var bytes = Encoding.UTF8.GetBytes(markdown);
        if (bytes.Length > 2 * 1024 * 1024 || _documentBytes + bytes.Length > 8 * 1024 * 1024)
            throw AgentTeamValidation.Invalid("document size");
        var safeName = SafeDocumentName(name);
        var reference = $"document_{Guid.NewGuid():N}";
        var attachment = new AgentMessageAttachment(Guid.NewGuid().ToString("D").ToLowerInvariant(),
            safeName, "text/markdown", AgentMessageAttachmentKind.File, bytes.LongLength, bytes);
        var draft = new DocumentDraft(reference, attachment, title,
            Convert.ToHexString(SHA256.HashData(bytes)).ToLowerInvariant(), false);
        _documents.Add(reference, draft);
        _documentBytes += bytes.Length;
        return draft;
    }

    public IReadOnlyList<AgentMessageAttachment> ReserveDocuments(IReadOnlyList<string> references)
    {
        if (references.Count > 8 || references.Distinct(StringComparer.Ordinal).Count() != references.Count)
            throw AgentTeamValidation.Invalid("document_refs");
        return references.Select(reference =>
        {
            if (!_documents.TryGetValue(reference, out var document) || document.Consumed)
                throw AgentTeamValidation.Invalid("document_refs");
            return document.Attachment;
        }).ToArray();
    }

    public void ConsumeDocuments(IReadOnlyList<string> references)
    {
        foreach (var reference in references)
            _documents[reference] = _documents[reference] with { Consumed = true };
    }

    public AgentToolExecutionResult? ReplayedSend(string callId, string signature)
    {
        if (!_sendReceipts.TryGetValue(callId, out var receipt)) return null;
        if (!string.Equals(receipt.Signature, signature, StringComparison.Ordinal))
            throw new AgentTeamException(AgentTeamError.Conflict,
                "The send tool call ID was reused with different arguments.");
        return receipt.Result;
    }

    public void RecordSend(string callId, string signature, AgentToolExecutionResult result) =>
        _sendReceipts.TryAdd(callId, new SendReceipt(signature, result));

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

    private static string SafeDocumentName(string name)
    {
        AgentTeamValidation.Text(name, nameof(name), 512);
        var fileName = Path.GetFileName(name.Trim());
        if (fileName is "." or ".." || fileName != name.Trim())
            throw AgentTeamValidation.Invalid(nameof(name));
        var invalid = Path.GetInvalidFileNameChars().ToHashSet();
        fileName = new string(fileName.Select(value => invalid.Contains(value) ? '_' : value)
            .ToArray());
        if (!fileName.EndsWith(".md", StringComparison.OrdinalIgnoreCase)) fileName += ".md";
        if (fileName.Length > 180) fileName = fileName[..177] + ".md";
        AgentTeamValidation.Text(fileName, nameof(name), 180);
        return fileName;
    }
}
