namespace ChatOS.Core.Domain;

public enum ConversationAttachmentKind
{
    Image,
    File,
    Audio,
}

public enum ConversationAttachmentOrigin
{
    File,
    PastedImage,
    PastedDocument,
    PastedText,
}

public sealed record ConversationAttachmentDraft(
    string Id,
    string Name,
    string MimeType,
    ConversationAttachmentKind Kind,
    ConversationAttachmentOrigin Origin,
    byte[] Data)
{
    public int Size => Data.Length;

    public static ConversationAttachmentDraft Create(
        string name,
        string mimeType,
        ConversationAttachmentKind kind,
        ConversationAttachmentOrigin origin,
        byte[] data) => new(
            Guid.NewGuid().ToString("N"),
            name,
            mimeType,
            kind,
            origin,
            data);
}

public sealed record ConversationAttachmentReference(
    string Id,
    string Name,
    string MimeType,
    int Size,
    ConversationAttachmentKind Kind,
    string? StorageProvider = null,
    string? Bucket = null,
    string? ObjectKey = null,
    string? Url = null,
    string? ViewUrl = null);
