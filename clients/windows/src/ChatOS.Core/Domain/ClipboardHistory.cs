namespace ChatOS.Core.Domain;

public enum ClipboardHistoryKind
{
    Text,
    Url,
    Files,
    Image,
}

public sealed record ClipboardHistoryEntry(
    Guid Id,
    ClipboardHistoryKind Kind,
    string Preview,
    string ContentHash,
    string? SourceApplication,
    DateTimeOffset CreatedAt,
    DateTimeOffset UpdatedAt,
    bool IsPinned,
    int ByteCount);

public sealed record ClipboardHistoryPayload(
    ClipboardHistoryKind Kind,
    string? Text = null,
    IReadOnlyList<string>? FilePaths = null,
    byte[]? ImageBytes = null)
{
    public int ByteCount => Kind switch
    {
        ClipboardHistoryKind.Image => ImageBytes?.Length ?? 0,
        ClipboardHistoryKind.Files => FilePaths?.Sum(static path =>
            System.Text.Encoding.UTF8.GetByteCount(path)) ?? 0,
        _ => System.Text.Encoding.UTF8.GetByteCount(Text ?? string.Empty),
    };
}
