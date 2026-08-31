using ChatOS.Connector.Relay;
using ChatOS.Connector.Workspaces;

namespace ChatOS.Connector.Tests;

public sealed class LocalProjectPathResolverTests
{
    [Fact]
    public void ResolvesLogicalAndAbsolutePathsWithinPairedWorkspace()
    {
        using var workspace = TestWorkspace.Create();
        Directory.CreateDirectory(Path.Combine(workspace.Root, "src"));
        var resolver = new LocalProjectPathResolver(workspace.Context);

        var logical = resolver.Resolve("local://connector/device-1/workspace-1/src");
        var absolute = resolver.Resolve(Path.Combine(workspace.Root, "src"));

        Assert.Equal("src", logical.RelativePath);
        Assert.Equal(Path.Combine(workspace.Root, "src"), logical.AbsolutePath);
        Assert.Equal("src", absolute.RelativePath);
        Assert.Null(absolute.LogicalPrefix);
    }

    [Theory]
    [InlineData("local://connector/other-device/workspace-1")]
    [InlineData("local://connector/device-1/missing")]
    [InlineData("relative/project")]
    public void RejectsUnknownOrNonAbsoluteProjectPaths(string path)
    {
        using var workspace = TestWorkspace.Create();
        var resolver = new LocalProjectPathResolver(workspace.Context);

        Assert.Throws<RelayRequestException>(() => resolver.Resolve(path));
    }

    [Fact]
    public void RejectsAbsolutePathOutsidePairedWorkspaces()
    {
        using var workspace = TestWorkspace.Create();
        var outside = Path.Combine(Path.GetTempPath(), $"chatos-outside-{Guid.NewGuid():N}");
        Directory.CreateDirectory(outside);
        try
        {
            var resolver = new LocalProjectPathResolver(workspace.Context);

            Assert.Throws<RelayRequestException>(() => resolver.Resolve(outside));
        }
        finally
        {
            Directory.Delete(outside, recursive: true);
        }
    }

    private sealed class TestWorkspace : IDisposable
    {
        private TestWorkspace(string root)
        {
            Root = root;
            var workspace = new ConnectorWorkspace(
                "workspace-1",
                "Test",
                root,
                "fingerprint");
            Context = new TestContext("device-1", [workspace]);
        }

        public string Root { get; }

        public TestContext Context { get; }

        public static TestWorkspace Create()
        {
            var root = Path.Combine(Path.GetTempPath(), $"chatos-paths-{Guid.NewGuid():N}");
            Directory.CreateDirectory(root);
            return new TestWorkspace(root);
        }

        public void Dispose()
        {
            if (Directory.Exists(Root))
            {
                Directory.Delete(Root, recursive: true);
            }
        }
    }

    private sealed class TestContext(
        string deviceId,
        IReadOnlyList<ConnectorWorkspace> workspaces) : IConnectorWorkspaceContext
    {
        public string? DeviceId => deviceId;

        public IReadOnlyList<ConnectorWorkspace> Workspaces => workspaces;

        public ConnectorWorkspace? Find(string workspaceId) =>
            workspaces.FirstOrDefault(value => value.Id == workspaceId);
    }
}
