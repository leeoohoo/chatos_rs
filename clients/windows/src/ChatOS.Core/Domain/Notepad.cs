namespace ChatOS.Core.Domain;

public sealed record NotepadNote(
    string Id,
    string Title,
    string Folder,
    IReadOnlyList<string> Tags,
    DateTimeOffset? CreatedAt,
    DateTimeOffset? UpdatedAt,
    string File);

public sealed record NotepadNoteDetail(
    NotepadNote Note,
    string Content);

public sealed record NotepadNoteDraft(
    string Folder,
    string Title,
    string Content,
    IReadOnlyList<string> Tags);

public sealed record NotepadNoteUpdate(
    string? Title = null,
    string? Content = null,
    string? Folder = null,
    IReadOnlyList<string>? Tags = null);

public enum NotepadEditorMode
{
    Preview,
    Edit,
    Split,
}

public sealed record NotepadFolderItem(string Path)
{
    public string Name => Path.Replace('\\', '/').Split('/').LastOrDefault() ?? Path;
}

public sealed record NotepadFolderRenameRequest(string From, string To);

public sealed record NotepadNoteCreationRequest(string Title, string? Folder = null);
