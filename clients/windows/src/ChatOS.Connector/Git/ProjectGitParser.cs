using ChatOS.Core.Domain;

namespace ChatOS.Connector.Git;

internal static class ProjectGitParser
{
    public static IReadOnlyList<ProjectGitChange> ParseChanges(
        string value,
        string repositoryRoot)
    {
        var tokens = value.Split('\0', StringSplitOptions.RemoveEmptyEntries);
        var changes = new List<ProjectGitChange>();
        for (var index = 0; index < tokens.Length; index++)
        {
            var token = tokens[index];
            if (token.Length < 3)
            {
                continue;
            }

            var indexStatus = token[..1];
            var workTreeStatus = token.Substring(1, 1);
            var path = token[3..];
            string? originalPath = null;
            if (indexStatus is "R" or "C" && index + 1 < tokens.Length)
            {
                originalPath = tokens[++index];
            }

            var absolute = Path.GetFullPath(Path.Combine(repositoryRoot, path));
            changes.Add(new ProjectGitChange(
                path,
                originalPath,
                absolute,
                indexStatus,
                workTreeStatus,
                ChangeKind(indexStatus, workTreeStatus)));
        }

        return changes
            .OrderByDescending(static change => change.Kind == ProjectGitChangeKind.Conflicted)
            .ThenBy(static change => change.Path, StringComparer.CurrentCultureIgnoreCase)
            .ToArray();
    }

    public static IReadOnlyList<ProjectGitBranch> ParseBranches(string value)
    {
        var branches = new List<ProjectGitBranch>();
        foreach (var line in value.Split(['\r', '\n'], StringSplitOptions.RemoveEmptyEntries))
        {
            var fields = line.Split('\0');
            if (fields.Length == 0 || string.IsNullOrWhiteSpace(fields[0]))
            {
                continue;
            }

            branches.Add(new ProjectGitBranch(
                fields[0],
                fields.Length > 1 && fields[1] == "*",
                fields.Length > 2 && !string.IsNullOrWhiteSpace(fields[2]) ? fields[2].Trim() : null));
        }

        return branches;
    }

    public static IReadOnlyList<ProjectGitCommit> ParseCommits(string value)
    {
        var commits = new List<ProjectGitCommit>();
        foreach (var rawRecord in value.Split('\u001e', StringSplitOptions.RemoveEmptyEntries))
        {
            var record = rawRecord.TrimStart('\r', '\n');
            var fields = record.Split('\0');
            if (fields.Length < 7)
            {
                continue;
            }

            commits.Add(new ProjectGitCommit(
                fields[0],
                fields[1],
                fields[2].Split(' ', StringSplitOptions.RemoveEmptyEntries),
                fields[3],
                DateTimeOffset.TryParse(fields[4], out var date) ? date : null,
                fields[5].Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries),
                fields[6]));
        }

        return commits;
    }

    private static ProjectGitChangeKind ChangeKind(string index, string workTree)
    {
        var pair = index + workTree;
        if (pair is "DD" or "AU" or "UD" or "UA" or "DU" or "AA" or "UU")
        {
            return ProjectGitChangeKind.Conflicted;
        }

        if (pair == "??")
        {
            return ProjectGitChangeKind.Untracked;
        }

        if (index == "D" || workTree == "D") return ProjectGitChangeKind.Deleted;
        if (index == "R" || workTree == "R") return ProjectGitChangeKind.Renamed;
        if (index == "C" || workTree == "C") return ProjectGitChangeKind.Copied;
        if (index == "A" || workTree == "A") return ProjectGitChangeKind.Added;
        if (index == "T" || workTree == "T") return ProjectGitChangeKind.TypeChanged;
        return ProjectGitChangeKind.Modified;
    }
}
