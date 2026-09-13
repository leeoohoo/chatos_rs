using System.Security.Cryptography;
using System.Text;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentMainChatSnapshotFactoryTests
{
    private readonly WindowsLocalAgentMainChatSnapshotFactory _factory = new();

    [Fact]
    public void SnapshotDigestsAreDeterministicAndSkillsAreCanonicallyOrdered()
    {
        var contact = Contact([
            new("skill-z", "Zeta", "Z instructions"),
            new("skill-a", "Alpha", "A instructions"),
        ]);

        var first = _factory.Make(contact, Project());
        var second = _factory.Make(contact with { Skills = contact.Skills.Reverse().ToArray() }, Project());

        Assert.Equal(first.Prompt.Digest, second.Prompt.Digest);
        Assert.Equal(first.Prompt.Revision, second.Prompt.Revision);
        Assert.Equal(first.Project?.Digest, second.Project?.Digest);
        var catalog = first.Prompt.Payload.GetProperty("skill_catalog_prompt").GetString();
        Assert.True(catalog!.IndexOf("skill-a", StringComparison.Ordinal)
            < catalog.IndexOf("skill-z", StringComparison.Ordinal));
    }

    [Fact]
    public void CapabilitySnapshotAllowsOnlyTheTwoMainChatTools()
    {
        var snapshots = _factory.Make(Contact([]), Project());

        Assert.Equal(
            ["ask_user", "create_local_task"],
            snapshots.Capabilities.Payload.GetProperty("allowed_tools")
                .EnumerateArray().Select(value => value.GetString()));
    }

    [Theory]
    [InlineData("Inspect /Users/alice/private")]
    [InlineData("Read C:\\secret\\token.txt")]
    [InlineData("Open file:///private/data")]
    public void LocalFilesystemPathsAreRejectedBeforeModelContextIsFrozen(string content)
    {
        var contact = Contact([new("skill-1", "Unsafe", content)]);

        Assert.Throws<InvalidDataException>(() => _factory.Make(contact, Project()));
    }

    [Fact]
    public void ProjectSnapshotContainsDesignContextButNoWorkspaceAuthority()
    {
        var snapshots = _factory.Make(Contact([]), Project());
        var payload = snapshots.Project!.Payload;

        Assert.Equal("project-1", payload.GetProperty("project_id").GetString());
        Assert.Equal("Calm portfolio", payload.GetProperty("design_context")
            .GetProperty("description").GetString());
        Assert.False(payload.TryGetProperty("workspace_id", out _));
        Assert.False(payload.TryGetProperty("relative_root", out _));
        Assert.False(payload.ToString().Contains("C:\\", StringComparison.Ordinal));
    }

    [Fact]
    public void UnicodeDigestUsesTheSharedRustCanonicalUtf8Representation()
    {
        var project = Project() with
        {
            Draft = new LocalProjectDraft("作品集", "workspace-1", "site", "留白设计"),
        };

        var snapshot = _factory.Make(Contact([]), project).Project!;
        const string canonical =
            "{\"design_context\":{\"description\":\"留白设计\"},\"project_id\":\"project-1\",\"project_name\":\"作品集\",\"snapshot_revision\":\"3\"}";
        var expected = $"sha256:{Convert.ToHexString(
            SHA256.HashData(Encoding.UTF8.GetBytes(canonical))).ToLowerInvariant()}";

        Assert.Equal(expected, snapshot.Digest);
    }

    private static LocalAgentContactRuntimeContext Contact(
        IReadOnlyList<LocalAgentContactSkill> skills) => new(
        "agent-1", "Design Agent", "Visual direction", "design", "Create polished UI",
        skills, "revision-1");

    private static LocalProjectRecord Project() => new(
        "project-1", "account-1",
        new LocalProjectDraft("Portfolio", "workspace-1", "site", "Calm portfolio"),
        3, LocalProjectStatus.Active, 1, 2);
}
