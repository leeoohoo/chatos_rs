using ChatOS.Connector.Relay;
using ChatOS.Connector.Workspaces;

namespace ChatOS.Connector.Tests;

public sealed class WorkspaceFilesystemTests
{
    [Theory]
    [InlineData("../outside")]
    [InlineData("folder/../../outside")]
    [InlineData("/absolute")]
    [InlineData("\\\\server\\share")]
    [InlineData("C:\\Windows")]
    [InlineData("file.txt:stream")]
    public void RejectsUnsafePaths(string path)
    {
        using var workspace = TestWorkspace.Create();
        var paths = new WorkspacePathGuard(workspace.Root);

        var error = Assert.Throws<RelayRequestException>(() => paths.ResolveWritable(path));

        Assert.Equal(400, error.StatusCode);
    }

    [Fact]
    public void RejectsRootMutation()
    {
        using var workspace = TestWorkspace.Create();
        var paths = new WorkspacePathGuard(workspace.Root);

        var error = Assert.Throws<RelayRequestException>(() => paths.ResolveWritable("."));

        Assert.Equal(400, error.StatusCode);
    }

    [Fact]
    public void RejectsSymbolicLinkTraversal()
    {
        using var workspace = TestWorkspace.Create();
        var outside = Path.Combine(Path.GetTempPath(), $"chatos-outside-{Guid.NewGuid():N}");
        Directory.CreateDirectory(outside);
        try
        {
            Directory.CreateSymbolicLink(Path.Combine(workspace.Root, "linked"), outside);
            var paths = new WorkspacePathGuard(workspace.Root);

            var error = Assert.Throws<RelayRequestException>(() => paths.ResolveExisting("linked"));

            Assert.Equal(400, error.StatusCode);
        }
        finally
        {
            Directory.Delete(outside, recursive: true);
        }
    }

    [Fact]
    public void SupportsCreateReadSearchMoveReplaceAndDelete()
    {
        using var workspace = TestWorkspace.Create();
        var filesystem = workspace.Filesystem();

        filesystem.CreateDirectory("docs/guides");
        filesystem.Write("docs/guides/readme.md", "Hello Connector\nsecond line", createOnly: true);
        var read = filesystem.Read("docs/guides/readme.md");
        Assert.Equal("Hello Connector\nsecond line", read.GetProperty("content").GetString());

        var entrySearch = filesystem.SearchEntries(".", "readme", 10, CancellationToken.None);
        Assert.Single(entrySearch.GetProperty("matches").EnumerateArray());
        var contentSearch = filesystem.SearchContent(".", "connector", 10, CancellationToken.None);
        var contentMatch = Assert.Single(contentSearch.GetProperty("matches").EnumerateArray());
        Assert.Equal(1, contentMatch.GetProperty("line").GetInt32());

        filesystem.Write("docs/replaced.md", "old", createOnly: true);
        var move = filesystem.Move(
            "docs/guides/readme.md",
            "docs/replaced.md",
            replaceExisting: true);
        Assert.True(move.GetProperty("replaced").GetBoolean());
        Assert.Equal("Hello Connector\nsecond line", File.ReadAllText(Path.Combine(workspace.Root, "docs", "replaced.md")));

        var delete = filesystem.Delete("docs", recursive: true);
        Assert.True(delete.GetProperty("deleted").GetBoolean());
        Assert.False(Directory.Exists(Path.Combine(workspace.Root, "docs")));
    }

    [Fact]
    public void DirectoryOnlyListingDoesNotExposeFilesOrLinks()
    {
        using var workspace = TestWorkspace.Create();
        Directory.CreateDirectory(Path.Combine(workspace.Root, "folder"));
        File.WriteAllText(Path.Combine(workspace.Root, "visible.txt"), "text");
        File.CreateSymbolicLink(
            Path.Combine(workspace.Root, "linked.txt"),
            Path.Combine(workspace.Root, "visible.txt"));

        var result = workspace.Filesystem().List(".", includeFiles: false);
        var entry = Assert.Single(result.GetProperty("entries").EnumerateArray());

        Assert.Equal("folder", entry.GetProperty("name").GetString());
        Assert.True(entry.GetProperty("is_dir").GetBoolean());
    }

    [Fact]
    public void RefusesToMoveDirectoryIntoItsOwnChild()
    {
        using var workspace = TestWorkspace.Create();
        Directory.CreateDirectory(Path.Combine(workspace.Root, "source"));

        var error = Assert.Throws<RelayRequestException>(() =>
            workspace.Filesystem().Move("source", "source/child", replaceExisting: false));

        Assert.Equal(400, error.StatusCode);
    }

    [Fact]
    public async Task WorkspaceRelayUsesWorkspaceIdentityAndReturnsDetailsInline()
    {
        using var workspace = TestWorkspace.Create();
        File.WriteAllText(Path.Combine(workspace.Root, "notes.txt"), "inline detail");
        var catalog = new StubCatalog(workspace.Model);
        var dispatcher = new RelayDispatcher(
            [new WorkspaceRelayHandler(catalog)],
            new AcceptingVerifier());

        var response = await dispatcher.DispatchAsync("""
            {
              "type": "workspace_filesystem_request",
              "request_id": "request-files",
              "workspace_id": "workspace-1",
              "headers": {},
              "body": { "operation": "read", "path": "notes.txt" }
            }
            """);

        Assert.Equal(200, response.Status);
        Assert.Equal("inline detail", response.Body.GetProperty("content").GetString());
    }

    private sealed class StubCatalog(ConnectorWorkspace workspace) : IConnectorWorkspaceCatalog
    {
        public ConnectorWorkspace? Find(string workspaceId) =>
            workspaceId == workspace.Id ? workspace : null;
    }

    private sealed class AcceptingVerifier : IRelayRequestVerifier
    {
        public Task VerifyAsync(RelayRequest request, CancellationToken cancellationToken) =>
            Task.CompletedTask;
    }

    private sealed class TestWorkspace : IDisposable
    {
        private TestWorkspace(string root)
        {
            Root = root;
            Model = new ConnectorWorkspace("workspace-1", "Test", root, "fingerprint");
        }

        public string Root { get; }

        public ConnectorWorkspace Model { get; }

        public static TestWorkspace Create()
        {
            var root = Path.Combine(Path.GetTempPath(), $"chatos-workspace-{Guid.NewGuid():N}");
            Directory.CreateDirectory(root);
            return new TestWorkspace(root);
        }

        public WorkspaceFilesystem Filesystem() => new(Model);

        public void Dispose()
        {
            if (Directory.Exists(Root))
            {
                Directory.Delete(Root, recursive: true);
            }
        }
    }
}
