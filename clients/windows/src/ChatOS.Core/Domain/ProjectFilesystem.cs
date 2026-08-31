namespace ChatOS.Core.Domain;

public sealed record ProjectFileEntry(
    string Name,
    string Path,
    string? DisplayPath,
    bool IsDirectory,
    bool IsWritable,
    long? Size,
    DateTimeOffset? ModifiedAt);

public sealed record ProjectDirectoryListing(
    string Path,
    string? ParentPath,
    bool IsWritable,
    IReadOnlyList<ProjectFileEntry> Entries,
    bool IsTruncated);

public sealed record ProjectFileContent(
    string Path,
    string? DisplayPath,
    string Name,
    string? ContentType,
    bool IsBinary,
    bool IsWritable,
    long Size,
    DateTimeOffset? ModifiedAt,
    string Content);

public sealed record ProjectFileContentMatch(
    string Path,
    string? DisplayPath,
    int Line,
    int Column,
    string Text);

public sealed record ProjectFileMoveResult(
    string? FromPath,
    string? ToPath,
    string? DisplayPath,
    string? Name,
    bool WasReplaced,
    bool WasMoved);

public enum ProjectFileExternalOpenMode
{
    Default,
    Reveal,
    Code,
}
