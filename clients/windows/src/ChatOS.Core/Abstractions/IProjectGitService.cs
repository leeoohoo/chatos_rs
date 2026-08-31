using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IProjectGitService
{
    Task<ProjectGitSnapshot> SnapshotAsync(
        string projectRoot,
        CancellationToken cancellationToken = default);

    Task InitializeRepositoryAsync(
        string projectRoot,
        CancellationToken cancellationToken = default);

    Task<ProjectGitDiff> DiffAsync(
        string projectRoot,
        ProjectGitChange change,
        bool staged,
        CancellationToken cancellationToken = default);

    Task StageAsync(
        string projectRoot,
        IReadOnlyList<string> paths,
        CancellationToken cancellationToken = default);

    Task UnstageAsync(
        string projectRoot,
        IReadOnlyList<string> paths,
        CancellationToken cancellationToken = default);

    Task CommitAsync(
        string projectRoot,
        string message,
        CancellationToken cancellationToken = default);

    Task SwitchBranchAsync(
        string projectRoot,
        string branch,
        CancellationToken cancellationToken = default);

    Task CreateBranchAsync(
        string projectRoot,
        string name,
        bool switchToBranch,
        CancellationToken cancellationToken = default);

    Task MergeBranchAsync(
        string projectRoot,
        string branch,
        CancellationToken cancellationToken = default);

    Task SaveRemoteAsync(
        string projectRoot,
        string? originalName,
        string name,
        string url,
        CancellationToken cancellationToken = default);

    Task RemoveRemoteAsync(
        string projectRoot,
        string name,
        CancellationToken cancellationToken = default);

    Task PullAsync(
        string projectRoot,
        CancellationToken cancellationToken = default);

    Task PushAsync(
        string projectRoot,
        CancellationToken cancellationToken = default);
}
