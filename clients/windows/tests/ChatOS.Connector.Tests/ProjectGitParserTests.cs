using ChatOS.Connector.Git;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class ProjectGitParserTests
{
    [Fact]
    public void ParsesPorcelainChangesIncludingRenameAndConflict()
    {
        var root = Path.GetTempPath();
        var value = " M src/App.cs\0?? notes.txt\0R  new.txt\0old.txt\0UU conflict.cs\0";

        var changes = ProjectGitParser.ParseChanges(value, root);

        Assert.Equal(4, changes.Count);
        Assert.Equal(ProjectGitChangeKind.Conflicted, changes[0].Kind);
        Assert.Equal("old.txt", changes.Single(value => value.Path == "new.txt").OriginalPath);
        Assert.True(changes.Single(value => value.Path == "notes.txt").HasWorkingTreeChanges);
        Assert.True(changes.Single(value => value.Path == "new.txt").HasStagedChanges);
    }

    [Fact]
    public void ParsesBranchesAndStructuredCommitRecords()
    {
        const string branches = "main\0*\0origin/main\nfeature\0 \0\n";
        const string commits =
            "aaaaaaaa\0aaaaaaa\0bbbbbbbb cccccccc\0Alice\02026-08-30T10:00:00+08:00\0HEAD -> main, origin/main\0Merge work\u001e";

        var parsedBranches = ProjectGitParser.ParseBranches(branches);
        var parsedCommits = ProjectGitParser.ParseCommits(commits);

        Assert.True(parsedBranches[0].IsCurrent);
        Assert.Equal("origin/main", parsedBranches[0].Upstream);
        Assert.True(parsedCommits[0].IsMerge);
        Assert.Equal("Merge work", parsedCommits[0].Subject);
        Assert.Equal(2, parsedCommits[0].Decorations.Count);
    }
}
