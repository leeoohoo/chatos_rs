using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

internal sealed record WindowsLocalConversationAttachmentSpec(
    string AttachmentId,
    string DisplayName,
    string MediaType,
    ulong ByteSize,
    string Sha256,
    string AuthorizedLocalRef,
    JsonElement Metadata);

public sealed class WindowsLocalAgentAttachmentVault
{
    internal const int MaximumAttachmentBytes = 20 * 1024 * 1024;
    internal const int MaximumReadBytes = 64 * 1024;
    private const string Prefix = "local-attachment:";
    private readonly string _root;

    public WindowsLocalAgentAttachmentVault() : this(Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "ChatOS", "WindowsClient", "LocalAgentAttachments")) { }

    internal WindowsLocalAgentAttachmentVault(string root) => _root = root;

    internal IReadOnlyList<WindowsLocalConversationAttachmentSpec> Authorize(
        IReadOnlyList<ConversationAttachmentDraft> drafts,
        string ownerUserId,
        string conversationId) => drafts.Select(draft =>
    {
        if (draft.Data.Length is 0 or > MaximumAttachmentBytes)
            throw new InvalidOperationException("The local attachment is empty or too large.");
        var token = Guid.NewGuid().ToString("D").ToLowerInvariant();
        var directory = ScopedDirectory(ownerUserId, conversationId);
        Directory.CreateDirectory(directory);
        var path = Path.Combine(directory, token);
        File.WriteAllBytes(path, draft.Data);
        return new WindowsLocalConversationAttachmentSpec(
            draft.Id, draft.Name, draft.MimeType, (ulong)draft.Data.Length,
            Hex(draft.Data), Prefix + token,
            JsonSerializer.SerializeToElement(new { kind = draft.Kind.ToString().ToLowerInvariant() }));
    }).ToArray();

    internal JsonElement Resolve(
        WindowsLocalConversationAttachmentRecord record,
        string ownerUserId,
        string conversationId,
        ulong offset,
        int limit)
    {
        if (!record.AuthorizedLocalRef.StartsWith(Prefix, StringComparison.Ordinal) ||
            record.ByteSize is 0 or > MaximumAttachmentBytes || limit is < 1 or > MaximumReadBytes ||
            offset > record.ByteSize)
            throw new InvalidOperationException("The local attachment request is invalid.");
        var token = record.AuthorizedLocalRef[Prefix.Length..];
        if (!Guid.TryParseExact(token, "D", out var parsed) ||
            parsed.ToString("D") != token)
            throw new InvalidOperationException("The local attachment reference is invalid.");
        var directory = Path.GetFullPath(ScopedDirectory(ownerUserId, conversationId));
        var path = Path.GetFullPath(Path.Combine(directory, token));
        if (!string.Equals(Path.GetDirectoryName(path), directory, StringComparison.OrdinalIgnoreCase))
            throw new InvalidOperationException("The local attachment reference is invalid.");
        if ((File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0)
            throw new InvalidOperationException("The local attachment reference is invalid.");
        var data = File.ReadAllBytes(path);
        if ((ulong)data.Length != record.ByteSize || data.Length > MaximumAttachmentBytes ||
            !string.Equals(Hex(data), record.Sha256, StringComparison.OrdinalIgnoreCase))
            throw new InvalidOperationException("The local attachment failed its integrity check.");
        var start = checked((int)offset);
        var count = Math.Min(limit, data.Length - start);
        var slice = data.AsSpan(start, count).ToArray();
        var text = TryUtf8(slice);
        var next = start + count < data.Length ? (ulong?)(start + count) : null;
        return JsonSerializer.SerializeToElement(new {
            display_name = record.DisplayName, media_type = record.MediaType,
            byte_size = record.ByteSize, sha256 = record.Sha256.ToLowerInvariant(),
            offset, next_offset = next, encoding = text is null ? "base64" : "utf-8",
            content = text ?? Convert.ToBase64String(slice),
        });
    }

    private string ScopedDirectory(string owner, string conversation) =>
        Path.Combine(_root, Hex(Encoding.UTF8.GetBytes(owner)), Hex(Encoding.UTF8.GetBytes(conversation)));

    private static string Hex(byte[] data) => Convert.ToHexString(SHA256.HashData(data)).ToLowerInvariant();

    private static string? TryUtf8(byte[] data)
    {
        try { return new UTF8Encoding(false, true).GetString(data); }
        catch (DecoderFallbackException) { return null; }
    }
}
