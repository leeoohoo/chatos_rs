using ChatOS.Connector.Relay;
using ChatOS.Connector.Workspaces;
using ChatOS.Core.Domain;
using System.Text.Json;

namespace ChatOS.Connector.Tests;

public sealed class WorkspaceFilesystemTests
{
    [Fact]
    public void DriveRootResolvesExistingDescendant()
    {
        var temp = Path.GetFullPath(Path.GetTempPath());
        var root = Path.GetPathRoot(temp);
        Assert.False(string.IsNullOrWhiteSpace(root));
        var paths = new WorkspacePathGuard(root!);

        var resolved = paths.ResolveExisting(Path.GetRelativePath(root!, temp));

        Assert.Equal(Path.TrimEndingDirectorySeparator(temp), resolved);
    }

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

        var error = Assert.Throws<WorkspaceRequestException>(() => paths.ResolveWritable(path));

        Assert.Equal(400, error.StatusCode);
    }

    [Fact]
    public void RejectsRootMutation()
    {
        using var workspace = TestWorkspace.Create();
        var paths = new WorkspacePathGuard(workspace.Root);

        var error = Assert.Throws<WorkspaceRequestException>(() => paths.ResolveWritable("."));

        Assert.Equal(400, error.StatusCode);
    }

    [Fact]
    [Trait("Category", "MachineIntegration")]
    public void RejectsSymbolicLinkTraversal()
    {
        using var workspace = TestWorkspace.Create();
        var outside = Path.Combine(Path.GetTempPath(), $"chatos-outside-{Guid.NewGuid():N}");
        Directory.CreateDirectory(outside);
        try
        {
            Directory.CreateSymbolicLink(Path.Combine(workspace.Root, "linked"), outside);
            var paths = new WorkspacePathGuard(workspace.Root);

            var error = Assert.Throws<WorkspaceRequestException>(() => paths.ResolveExisting("linked"));

            Assert.Equal(400, error.StatusCode);
        }
        finally
        {
            Directory.Delete(outside, recursive: true);
        }
    }

    [Fact]
    public void SupportsCreateReadSearchCopyMoveReplaceAndDelete()
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

        var copy = filesystem.Copy(
            "docs/guides/readme.md",
            "docs/copied.md",
            replaceExisting: false,
            CancellationToken.None);
        Assert.True(copy.GetProperty("copied").GetBoolean());
        Assert.False(copy.GetProperty("replaced").GetBoolean());
        Assert.Equal("Hello Connector\nsecond line", File.ReadAllText(
            Path.Combine(workspace.Root, "docs", "copied.md")));

        filesystem.Write("docs/guides/readme.md", "updated source", createOnly: false);
        var replacedCopy = filesystem.Copy(
            "docs/guides/readme.md",
            "docs/copied.md",
            replaceExisting: true,
            CancellationToken.None);
        Assert.True(replacedCopy.GetProperty("replaced").GetBoolean());
        Assert.Equal("updated source", File.ReadAllText(
            Path.Combine(workspace.Root, "docs", "copied.md")));
        Assert.Equal("updated source", File.ReadAllText(
            Path.Combine(workspace.Root, "docs", "guides", "readme.md")));

        filesystem.Write("docs/replaced.md", "old", createOnly: true);
        var move = filesystem.Move(
            "docs/guides/readme.md",
            "docs/replaced.md",
            replaceExisting: true);
        Assert.True(move.GetProperty("replaced").GetBoolean());
        Assert.Equal("updated source", File.ReadAllText(Path.Combine(workspace.Root, "docs", "replaced.md")));

        var delete = filesystem.Delete("docs", recursive: true);
        Assert.True(delete.GetProperty("deleted").GetBoolean());
        Assert.False(Directory.Exists(Path.Combine(workspace.Root, "docs")));
    }

    [Fact]
    public void MoveSupportsWindowsCaseOnlyFileAndDirectoryRenames()
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        using var workspace = TestWorkspace.Create();
        var filesystem = workspace.Filesystem();
        filesystem.Write("note.txt", "CASE-RENAME", createOnly: true);
        filesystem.CreateDirectory("folder");

        var fileMove = filesystem.Move("note.txt", "NOTE.txt", replaceExisting: false);
        var directoryMove = filesystem.Move("folder", "FOLDER", replaceExisting: false);

        Assert.True(fileMove.GetProperty("moved").GetBoolean());
        Assert.False(fileMove.GetProperty("replaced").GetBoolean());
        Assert.Equal("note.txt", fileMove.GetProperty("from_path").GetString());
        Assert.Equal("NOTE.txt", fileMove.GetProperty("to_path").GetString());
        Assert.True(directoryMove.GetProperty("moved").GetBoolean());
        Assert.Equal("FOLDER", directoryMove.GetProperty("to_path").GetString());
        Assert.Equal("CASE-RENAME", File.ReadAllText(Path.Combine(workspace.Root, "NOTE.txt")));
        Assert.Contains("NOTE.txt", Directory.EnumerateFiles(workspace.Root).Select(Path.GetFileName));
        Assert.Contains("FOLDER", Directory.EnumerateDirectories(workspace.Root).Select(Path.GetFileName));
        Assert.DoesNotContain("note.txt", Directory.EnumerateFiles(workspace.Root).Select(Path.GetFileName));
        Assert.DoesNotContain("folder", Directory.EnumerateDirectories(workspace.Root).Select(Path.GetFileName));

        var exactRetry = filesystem.Move("NOTE.txt", "NOTE.txt", replaceExisting: false);
        Assert.False(exactRetry.GetProperty("moved").GetBoolean());
        Assert.Equal("CASE-RENAME", File.ReadAllText(Path.Combine(workspace.Root, "NOTE.txt")));
    }

    [Fact]
    public void WriteReturnsTruthfulBoundedDiffAndHashes()
    {
        using var workspace = TestWorkspace.Create();
        var filesystem = workspace.Filesystem();

        var created = filesystem.Write("note.txt", "FIRST\nMARKER", createOnly: true);
        var createdChange = created.GetProperty("change");
        Assert.True(createdChange.GetProperty("changed").GetBoolean());
        Assert.True(createdChange.GetProperty("diff_available").GetBoolean());
        Assert.False(createdChange.GetProperty("diff_truncated").GetBoolean());
        Assert.Equal(JsonValueKind.Null, createdChange.GetProperty("unavailable_reason").ValueKind);
        Assert.Equal(
            "--- /dev/null\n+++ b/note.txt\n@@ -0,0 +1,2 @@\n+FIRST\n+MARKER\n",
            createdChange.GetProperty("unified_diff").GetString());
        Assert.Equal(64, createdChange.GetProperty("before_sha256").GetString()!.Length);
        Assert.Equal(64, createdChange.GetProperty("after_sha256").GetString()!.Length);

        var updated = filesystem.Write("note.txt", "SECOND\nMARKER", createOnly: false);
        var updatedChange = updated.GetProperty("change");
        Assert.True(updatedChange.GetProperty("changed").GetBoolean());
        Assert.Equal(
            "--- a/note.txt\n+++ b/note.txt\n@@ -1,2 +1,2 @@\n-FIRST\n-MARKER\n+SECOND\n+MARKER\n",
            updatedChange.GetProperty("unified_diff").GetString());
        Assert.NotEqual(
            updatedChange.GetProperty("before_sha256").GetString(),
            updatedChange.GetProperty("after_sha256").GetString());

        var unchanged = filesystem.Write("note.txt", "SECOND\nMARKER", createOnly: false);
        var unchangedChange = unchanged.GetProperty("change");
        Assert.False(unchangedChange.GetProperty("changed").GetBoolean());
        Assert.Equal(string.Empty, unchangedChange.GetProperty("unified_diff").GetString());
        Assert.Equal(
            unchangedChange.GetProperty("before_sha256").GetString(),
            unchangedChange.GetProperty("after_sha256").GetString());
    }

    [Fact]
    public void WriteReportsUnavailableDiffForInvalidUtf8PreviousContent()
    {
        using var workspace = TestWorkspace.Create();
        File.WriteAllBytes(Path.Combine(workspace.Root, "binary.txt"), [0xC3, 0x28]);

        var result = workspace.Filesystem().Write("binary.txt", "now text", createOnly: false);
        var change = result.GetProperty("change");

        Assert.True(change.GetProperty("changed").GetBoolean());
        Assert.False(change.GetProperty("diff_available").GetBoolean());
        Assert.Equal("previous_file_not_utf8", change.GetProperty("unavailable_reason").GetString());
        Assert.Equal(JsonValueKind.Null, change.GetProperty("unified_diff").ValueKind);
        Assert.Equal("now text", File.ReadAllText(Path.Combine(workspace.Root, "binary.txt")));
    }

    [Fact]
    public void WriteMarksOversizedDiffAsTruncated()
    {
        using var workspace = TestWorkspace.Create();
        var filesystem = workspace.Filesystem();
        filesystem.Write("large.txt", new string('a', 40_000), createOnly: true);

        var result = filesystem.Write("large.txt", new string('b', 40_000), createOnly: false);
        var change = result.GetProperty("change");
        var diff = change.GetProperty("unified_diff").GetString();

        Assert.True(change.GetProperty("diff_available").GetBoolean());
        Assert.True(change.GetProperty("diff_truncated").GetBoolean());
        Assert.NotNull(diff);
        Assert.True(diff.Length <= WorkspaceFilesystem.MaximumWriteDiffCharacters);
    }

    [Fact]
    public void CopyIsAtomicOnCancellationAndRejectsOverlappingDirectoryTargets()
    {
        using var workspace = TestWorkspace.Create();
        var filesystem = workspace.Filesystem();
        filesystem.CreateDirectory("source/nested");
        filesystem.Write("source/nested/file.txt", "content", createOnly: true);
        using var cancellation = new CancellationTokenSource();
        cancellation.Cancel();

        Assert.Throws<OperationCanceledException>(() => filesystem.Copy(
            "source", "cancelled-copy", replaceExisting: false, cancellation.Token));
        Assert.False(Directory.Exists(Path.Combine(workspace.Root, "cancelled-copy")));

        var overlap = Assert.Throws<WorkspaceRequestException>(() => filesystem.Copy(
            "source", "source/child", replaceExisting: false, CancellationToken.None));
        Assert.Equal(400, overlap.StatusCode);
        Assert.False(Directory.Exists(Path.Combine(workspace.Root, "source", "child")));
    }

    [Fact]
    public void DirectoryOnlyListingDoesNotExposeFiles()
    {
        using var workspace = TestWorkspace.Create();
        Directory.CreateDirectory(Path.Combine(workspace.Root, "folder"));
        File.WriteAllText(Path.Combine(workspace.Root, "visible.txt"), "text");

        var result = workspace.Filesystem().List(".", includeFiles: false);
        var entry = Assert.Single(result.GetProperty("entries").EnumerateArray());

        Assert.Equal("folder", entry.GetProperty("name").GetString());
        Assert.True(entry.GetProperty("is_dir").GetBoolean());
        Assert.False(result.GetProperty("incomplete").GetBoolean());
        Assert.Equal(0, result.GetProperty("omitted_reparse_points").GetInt32());
    }

    [Fact]
    public void ContentSearchMarksLockedFileAsIncompleteInsteadOfClaimingNoMatch()
    {
        using var workspace = TestWorkspace.Create();
        var path = Path.Combine(workspace.Root, "locked.txt");
        File.WriteAllText(path, "unique-search-marker");
        using var locked = new FileStream(
            path,
            FileMode.Open,
            FileAccess.ReadWrite,
            FileShare.None);

        var result = workspace.Filesystem().SearchContent(
            ".", "unique-search-marker", 10, CancellationToken.None);

        Assert.Empty(result.GetProperty("matches").EnumerateArray());
        Assert.True(result.GetProperty("truncated").GetBoolean());
        Assert.True(result.GetProperty("incomplete").GetBoolean());
    }

    [Fact]
    public void ReadAndContentSearchTreatInvalidUtf8AsBinaryWithoutInventingText()
    {
        using var workspace = TestWorkspace.Create();
        var bytes = new byte[] { (byte)'A', 0xC3, 0x28, (byte)'B' };
        File.WriteAllBytes(Path.Combine(workspace.Root, "invalid.txt"), bytes);
        var filesystem = workspace.Filesystem();

        var read = filesystem.Read("invalid.txt");

        Assert.True(read.GetProperty("is_binary").GetBoolean());
        Assert.Equal(Convert.ToBase64String(bytes), read.GetProperty("content").GetString());
        var search = filesystem.SearchContent(".", "\uFFFD", 10, CancellationToken.None);
        Assert.Empty(search.GetProperty("matches").EnumerateArray());
        Assert.Equal(0, search.GetProperty("scanned_files").GetInt32());
        Assert.False(search.GetProperty("incomplete").GetBoolean());
    }

    [Fact]
    public void EntryEnumerationReportsMissingDirectoryAsIncomplete()
    {
        using var workspace = TestWorkspace.Create();
        var missing = Path.Combine(workspace.Root, "missing");

        var complete = WorkspaceFilesystem.TryEnumerateEntries(missing, out var entries);

        Assert.False(complete);
        Assert.Empty(entries);
    }

    [Fact]
    [Trait("Category", "MachineIntegration")]
    public void DirectoryOnlyListingDoesNotExposeSymbolicLinks()
    {
        using var workspace = TestWorkspace.Create();
        Directory.CreateDirectory(Path.Combine(workspace.Root, "folder"));
        File.WriteAllText(Path.Combine(workspace.Root, "visible.txt"), "text");
        Directory.CreateSymbolicLink(
            Path.Combine(workspace.Root, "linked-folder"),
            Path.Combine(workspace.Root, "folder"));
        File.CreateSymbolicLink(
            Path.Combine(workspace.Root, "linked.txt"),
            Path.Combine(workspace.Root, "visible.txt"));

        var result = workspace.Filesystem().List(".", includeFiles: false);
        var entry = Assert.Single(result.GetProperty("entries").EnumerateArray());

        Assert.Equal("folder", entry.GetProperty("name").GetString());
        Assert.True(entry.GetProperty("is_dir").GetBoolean());
        Assert.True(result.GetProperty("incomplete").GetBoolean());
        Assert.Equal(2, result.GetProperty("omitted_reparse_points").GetInt32());

        var entrySearch = workspace.Filesystem().SearchEntries(
            ".", "definitely-absent", 10, CancellationToken.None);
        Assert.Empty(entrySearch.GetProperty("matches").EnumerateArray());
        Assert.True(entrySearch.GetProperty("truncated").GetBoolean());
        Assert.True(entrySearch.GetProperty("incomplete").GetBoolean());

        var contentSearch = workspace.Filesystem().SearchContent(
            ".", "definitely-absent", 10, CancellationToken.None);
        Assert.Empty(contentSearch.GetProperty("matches").EnumerateArray());
        Assert.True(contentSearch.GetProperty("truncated").GetBoolean());
        Assert.True(contentSearch.GetProperty("incomplete").GetBoolean());
    }

    [Fact]
    public void RefusesToMoveDirectoryIntoItsOwnChild()
    {
        using var workspace = TestWorkspace.Create();
        Directory.CreateDirectory(Path.Combine(workspace.Root, "source"));

        var error = Assert.Throws<WorkspaceRequestException>(() =>
            workspace.Filesystem().Move("source", "source/child", replaceExisting: false));

        Assert.Equal(400, error.StatusCode);
    }

    [Fact]
    public void NonEmptyDirectoryFailureUsesStablePathFreeWorkspaceError()
    {
        using var workspace = TestWorkspace.Create();
        var filesystem = workspace.Filesystem();
        filesystem.CreateDirectory("docs");
        filesystem.Write("docs/note.txt", "content", createOnly: true);

        var error = Assert.Throws<WorkspaceRequestException>(() =>
            filesystem.Delete("docs", recursive: false));

        Assert.Equal(409, error.StatusCode);
        Assert.Equal("workspace_directory_not_empty", error.Code);
        Assert.Equal("Directory is not empty.", error.Message);
        Assert.DoesNotContain(workspace.Root, error.Message, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public async Task ProjectFilesystemServiceReadsAndSearchesTheLocalWorkspace()
    {
        using var workspace = TestWorkspace.Create();
        Directory.CreateDirectory(Path.Combine(workspace.Root, "src"));
        await File.WriteAllTextAsync(
            Path.Combine(workspace.Root, "src", "App.cs"),
            "class LocalAgentHost {}");
        var service = workspace.ProjectFilesystemService();

        var listing = await service.ListEntriesAsync(workspace.LogicalRoot);
        var content = await service.ReadFileAsync($"{workspace.LogicalRoot}/src/App.cs");
        var names = await service.SearchEntriesAsync(workspace.LogicalRoot, "App", 10);
        var matches = await service.SearchContentAsync(workspace.LogicalRoot, "LocalAgent", 10);

        Assert.Equal(workspace.LogicalRoot, listing.Path);
        Assert.Equal($"{workspace.LogicalRoot}/src", Assert.Single(listing.Entries).Path);
        Assert.Equal("class LocalAgentHost {}", content.Content);
        Assert.Equal("text/x-csharp", content.ContentType);
        Assert.Equal($"{workspace.LogicalRoot}/src/App.cs", Assert.Single(names).Path);
        Assert.Equal(1, Assert.Single(matches).Line);
    }

    [Fact]
    public async Task ProjectFilesystemServiceMutatesOnlyTheResolvedLocalWorkspace()
    {
        using var workspace = TestWorkspace.Create();
        var service = workspace.ProjectFilesystemService();

        await service.CreateDirectoryAsync(workspace.LogicalRoot, "docs");
        await service.CreateFileAsync($"{workspace.LogicalRoot}/docs", "draft.md");
        await service.WriteFileAsync($"{workspace.LogicalRoot}/docs/draft.md", "local content");
        var moved = await service.MoveEntryAsync(
            $"{workspace.LogicalRoot}/docs/draft.md",
            workspace.LogicalRoot,
            "README.md");
        await service.DeleteEntryAsync($"{workspace.LogicalRoot}/docs", recursive: false);

        Assert.True(moved.WasMoved);
        Assert.Equal($"{workspace.LogicalRoot}/README.md", moved.ToPath);
        Assert.Equal("local content", await File.ReadAllTextAsync(
            Path.Combine(workspace.Root, "README.md")));
        Assert.False(Directory.Exists(Path.Combine(workspace.Root, "docs")));
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

        public string LogicalRoot => "local://connector/device/workspace-1";

        public static TestWorkspace Create()
        {
            var root = Path.Combine(Path.GetTempPath(), $"chatos-workspace-{Guid.NewGuid():N}");
            Directory.CreateDirectory(root);
            return new TestWorkspace(root);
        }

        public WorkspaceFilesystem Filesystem() => new(Model);

        public WindowsProjectFilesystemService ProjectFilesystemService() =>
            new(new TestPathResolver(this));

        public void Dispose()
        {
            if (Directory.Exists(Root))
            {
                Directory.Delete(Root, recursive: true);
            }
        }

        private sealed class TestPathResolver(TestWorkspace workspace) : ILocalProjectPathResolver
        {
            public ResolvedLocalProjectPath Resolve(string rawPath)
            {
                if (!rawPath.StartsWith(workspace.LogicalRoot, StringComparison.Ordinal))
                {
                    throw new RelayRequestException(404, "outside test workspace");
                }
                var suffix = rawPath[workspace.LogicalRoot.Length..].TrimStart('/');
                var relative = suffix.Length == 0 ? "." : suffix;
                var guard = new WorkspacePathGuard(workspace.Root);
                var absolute = guard.ResolveExisting(relative);
                return new ResolvedLocalProjectPath(
                    workspace.Model,
                    guard.RelativePath(absolute),
                    absolute,
                    workspace.LogicalRoot);
            }
        }
    }
}
