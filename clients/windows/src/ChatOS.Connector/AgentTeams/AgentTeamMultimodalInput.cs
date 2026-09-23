using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.AgentTeams;

internal sealed record AgentMultimodalAttachment(
    string RoomId,
    string AttachmentId,
    string Reference,
    string Name,
    string MimeType,
    AgentMessageAttachmentKind Kind,
    byte[] Data);

internal readonly record struct AgentMultimodalPolicy(
    int MaximumCount,
    long MaximumItemBytes,
    long MaximumTotalBytes)
{
    public static AgentMultimodalPolicy Default { get; } = new(
        MaximumCount: 8,
        MaximumItemBytes: 8L * 1024 * 1024,
        MaximumTotalBytes: 16L * 1024 * 1024);
}

internal static class AgentTeamMultimodalInput
{
    private static readonly HashSet<string> ImageMimeTypes = new(StringComparer.Ordinal)
    {
        "image/png",
        "image/jpeg",
        "image/gif",
        "image/webp",
    };

    public static async Task<IReadOnlyList<AgentMultimodalAttachment>> LoadAsync(
        IAgentTeamStore store,
        string ownerUserId,
        IEnumerable<AgentMessage> messages,
        AgentRunReferenceVault references,
        CancellationToken cancellationToken,
        AgentMultimodalPolicy? policy = null)
    {
        var limits = policy ?? AgentMultimodalPolicy.Default;
        if (limits.MaximumCount < 1 || limits.MaximumItemBytes < 1 ||
            limits.MaximumTotalBytes < 1)
        {
            throw new ArgumentOutOfRangeException(nameof(policy));
        }

        var output = new List<AgentMultimodalAttachment>();
        var seen = new HashSet<string>(StringComparer.Ordinal);
        long totalBytes = 0;
        var inspected = 0;
        foreach (var message in messages)
        {
            foreach (var metadata in message.Attachments)
            {
                var key = $"{message.RoomId}\n{metadata.Id}";
                if (!seen.Add(key)) continue;
                var mimeType = NormalizeMimeType(metadata.MimeType);
                if (!IsSupported(metadata.Kind, mimeType)) continue;
                if (++inspected > limits.MaximumCount) return output;
                if (metadata.ByteCount <= 0 || metadata.ByteCount > limits.MaximumItemBytes ||
                    totalBytes + metadata.ByteCount > limits.MaximumTotalBytes)
                {
                    continue;
                }

                var payload = await store.GetMessageAttachmentForMessageAsync(
                    ownerUserId, message.RoomId, message.Id, metadata.Id, cancellationToken)
                    .ConfigureAwait(false);
                if (payload is null || payload.Id != metadata.Id ||
                    payload.Kind != metadata.Kind || payload.ByteCount != metadata.ByteCount ||
                    NormalizeMimeType(payload.MimeType) != mimeType || !payload.HasPayload ||
                    payload.Data.LongLength > limits.MaximumItemBytes ||
                    totalBytes + payload.Data.LongLength > limits.MaximumTotalBytes ||
                    !HasExpectedSignature(payload.Data, mimeType))
                {
                    continue;
                }

                var reference = references.AttachmentReference(
                    message.RoomId, message.Id, metadata.Id);
                output.Add(new AgentMultimodalAttachment(
                    message.RoomId,
                    metadata.Id,
                    reference,
                    SafeFileName(metadata.Name, mimeType),
                    mimeType,
                    metadata.Kind,
                    payload.Data));
                totalBytes += payload.Data.LongLength;
            }
        }
        return output;
    }

    public static Dictionary<string, object> UserMessage(
        string text,
        IReadOnlyList<AgentMultimodalAttachment> attachments)
    {
        if (attachments.Count == 0)
            return new Dictionary<string, object> { ["role"] = "user", ["content"] = text };

        var content = new List<object>
        {
            new Dictionary<string, object> { ["type"] = "input_text", ["text"] = text },
        };
        foreach (var attachment in attachments)
        {
            content.Add(new Dictionary<string, object>
            {
                ["type"] = "input_text",
                ["text"] = $"[multimodal attachment_ref={attachment.Reference} name={attachment.Name}]",
            });
            var dataUrl = $"data:{attachment.MimeType};base64,{Convert.ToBase64String(attachment.Data)}";
            content.Add(attachment.Kind == AgentMessageAttachmentKind.Image
                ? new Dictionary<string, object>
                {
                    ["type"] = "input_image",
                    ["image_url"] = dataUrl,
                    ["detail"] = "auto",
                }
                : new Dictionary<string, object>
                {
                    ["type"] = "input_file",
                    ["filename"] = attachment.Name,
                    ["file_data"] = dataUrl,
                });
        }
        return new Dictionary<string, object> { ["role"] = "user", ["content"] = content };
    }

    public static string SafeFileName(string name, string mimeType)
    {
        var leaf = name.Trim().Split(['/', '\\'], StringSplitOptions.RemoveEmptyEntries)
            .LastOrDefault() ?? string.Empty;
        leaf = new string(leaf.Select(value => char.IsControl(value) || value is ':' or '"' or '<' or '>' or '|'
            ? '_'
            : value).ToArray()).Trim();
        if (leaf.Length > 120) leaf = leaf[..120];
        if (!string.IsNullOrWhiteSpace(leaf)) return leaf;
        return mimeType switch
        {
            "application/pdf" => "attachment.pdf",
            "image/jpeg" => "attachment.jpg",
            "image/gif" => "attachment.gif",
            "image/webp" => "attachment.webp",
            _ => "attachment.png",
        };
    }

    public static bool IsTextMimeType(string value)
    {
        var mimeType = NormalizeMimeType(value);
        return mimeType.StartsWith("text/", StringComparison.Ordinal) || mimeType is
            "application/json" or "application/xml" or "application/javascript" or
            "application/yaml" or "application/toml" or "application/sql";
    }

    private static bool IsSupported(AgentMessageAttachmentKind kind, string mimeType) =>
        kind == AgentMessageAttachmentKind.Image && ImageMimeTypes.Contains(mimeType) ||
        kind == AgentMessageAttachmentKind.File && mimeType == "application/pdf";

    private static string NormalizeMimeType(string value) =>
        value.Split(';', 2)[0].Trim().ToLowerInvariant();

    private static bool HasExpectedSignature(byte[] data, string mimeType)
    {
        ReadOnlySpan<byte> bytes = data;
        return mimeType switch
        {
            "image/png" => bytes.Length >= 8 && bytes[0] == 0x89 &&
                bytes[1..8].SequenceEqual("PNG\r\n\x1a\n"u8),
            "image/jpeg" => bytes.Length >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff,
            "image/gif" => bytes.StartsWith("GIF87a"u8) || bytes.StartsWith("GIF89a"u8),
            "image/webp" => bytes.Length >= 12 && bytes[..4].SequenceEqual("RIFF"u8) &&
                bytes.Slice(8, 4).SequenceEqual("WEBP"u8),
            "application/pdf" => bytes.StartsWith("%PDF-"u8),
            _ => false,
        };
    }
}
