namespace ChatOS.Core.Domain;

public enum ProjectGitChangeKind
{
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Conflicted,
    TypeChanged,
}

public sealed record ProjectGitChange(
    string Path,
    string? OriginalPath,
    string AbsolutePath,
    string IndexStatus,
    string WorkTreeStatus,
    ProjectGitChangeKind Kind)
{
    public bool HasStagedChanges => IndexStatus is not (" " or "?");

    public bool HasWorkingTreeChanges =>
        WorkTreeStatus != " " || (IndexStatus == "?" && WorkTreeStatus == "?");
}

public sealed record ProjectGitBranch(
    string Name,
    bool IsCurrent,
    string? Upstream);

public sealed record ProjectGitCommit(
    string Id,
    string ShortId,
    IReadOnlyList<string> ParentIds,
    string Author,
    DateTimeOffset? AuthoredAt,
    IReadOnlyList<string> Decorations,
    string Subject)
{
    public bool IsMerge => ParentIds.Count > 1;
}

public sealed record ProjectGitRemote(
    string Name,
    string Url);

public sealed record ProjectGitSnapshot(
    bool IsRepository,
    string? RepositoryRoot,
    string? CurrentBranch,
    string? DetachedCommit,
    string? Upstream,
    int AheadCount,
    int BehindCount,
    bool HasHead,
    IReadOnlyList<ProjectGitChange> Changes,
    IReadOnlyList<ProjectGitBranch> Branches,
    IReadOnlyList<ProjectGitCommit> Commits,
    IReadOnlyList<ProjectGitRemote> Remotes)
{
    public static ProjectGitSnapshot Unavailable { get; } = new(
        false,
        null,
        null,
        null,
        null,
        0,
        0,
        false,
        Array.Empty<ProjectGitChange>(),
        Array.Empty<ProjectGitBranch>(),
        Array.Empty<ProjectGitCommit>(),
        Array.Empty<ProjectGitRemote>());
}

public sealed record ProjectGitDiff(
    string Path,
    bool IsStaged,
    string Content);

public sealed record ProjectGitRemoteDraft(
    string? OriginalName,
    string Name,
    string Url);

public sealed record ProjectGitBranchDraft(
    string Name,
    bool SwitchToBranch);

public sealed record ProjectGitDiffRequest(
    ProjectGitChange Change,
    bool Staged);
