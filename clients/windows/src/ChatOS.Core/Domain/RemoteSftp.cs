namespace ChatOS.Core.Domain;

public sealed record RemoteFileEntry(
    string Name,
    string FullPath,
    bool IsDirectory,
    bool IsSymbolicLink,
    long Size,
    DateTimeOffset LastModifiedAt);
