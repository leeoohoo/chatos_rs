using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IProjectFilesystemService
{
    Task<ProjectDirectoryListing> ListEntriesAsync(
        string path,
        bool forceRefresh = false,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<ProjectFileEntry>> SearchEntriesAsync(
        string path,
        string query,
        int limit = 100,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<ProjectFileContentMatch>> SearchContentAsync(
        string path,
        string query,
        int limit = 100,
        CancellationToken cancellationToken = default);

    Task<ProjectFileContent> ReadFileAsync(
        string path,
        CancellationToken cancellationToken = default);

    Task WriteFileAsync(
        string path,
        string content,
        CancellationToken cancellationToken = default);

    Task CreateFileAsync(
        string parentPath,
        string name,
        CancellationToken cancellationToken = default);

    Task CreateDirectoryAsync(
        string parentPath,
        string name,
        CancellationToken cancellationToken = default);

    Task DeleteEntryAsync(
        string path,
        bool recursive,
        CancellationToken cancellationToken = default);

    Task<ProjectFileMoveResult> MoveEntryAsync(
        string sourcePath,
        string targetParentPath,
        string? targetName = null,
        bool replaceExisting = false,
        CancellationToken cancellationToken = default);

    Task OpenExternallyAsync(
        string path,
        ProjectFileExternalOpenMode mode,
        CancellationToken cancellationToken = default);
}
