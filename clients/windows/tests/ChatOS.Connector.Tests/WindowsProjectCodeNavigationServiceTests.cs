using ChatOS.Connector.Relay;
using ChatOS.Connector.Workspaces;

namespace ChatOS.Connector.Tests;

public sealed class WindowsProjectCodeNavigationServiceTests
{
    [Fact]
    public async Task FindsDefinitionAndReferencesInsideProjectBoundary()
    {
        using var context = TestContext.Create();
        Directory.CreateDirectory(Path.Combine(context.Root, "src"));
        await File.WriteAllTextAsync(
            Path.Combine(context.Root, "src", "Widget.cs"),
            "namespace Demo;\npublic class Widget { }\n");
        await File.WriteAllTextAsync(
            Path.Combine(context.Root, "src", "App.cs"),
            "namespace Demo;\nvar widget = new Widget();\nWidget Run(Widget value) => value;\n");

        var definition = await context.Service.DefinitionAsync(
            context.LogicalRoot,
            context.LogicalRoot + "/src/App.cs",
            2,
            19);
        var references = await context.Service.ReferencesAsync(
            context.LogicalRoot,
            context.LogicalRoot + "/src/App.cs",
            2,
            19);

        Assert.Equal("Widget", definition.Token);
        Assert.Equal("src/Widget.cs", definition.Locations[0].RelativePath);
        Assert.Contains(references.Locations, value => value.RelativePath == "src/App.cs" && value.Line == 3);
        Assert.DoesNotContain(references.Locations, value => value.RelativePath == "src/App.cs" && value.Line == 2 && value.Column <= 19 && value.EndColumn >= 19);
    }

    [Theory]
    [InlineData(0, 1)]
    [InlineData(1, 2)]
    public void TokenSelectionHandlesInvalidAndBoundaryPositions(int line, int column)
    {
        var token = WindowsProjectCodeNavigationService.TokenAt(line, column, line == 0 ? "Widget" : " Widget ");

        if (line == 0) Assert.Null(token);
        else Assert.Equal("Widget", token);
    }

    [Fact]
    public async Task RejectsFileOutsideSelectedProjectEvenWithinSameWorkspace()
    {
        using var context = TestContext.Create();
        Directory.CreateDirectory(Path.Combine(context.Root, "project"));
        await File.WriteAllTextAsync(Path.Combine(context.Root, "outside.cs"), "class Outside {}\n");

        var error = await Assert.ThrowsAsync<RelayRequestException>(() =>
            context.Service.DefinitionAsync(
                context.LogicalRoot + "/project",
                context.LogicalRoot + "/outside.cs",
                1,
                7));

        Assert.Equal(400, error.StatusCode);
    }

    private sealed class TestContext : IDisposable
    {
        private TestContext(string root)
        {
            Root = root;
            LogicalRoot = "local://connector/device-1/workspace-1";
            var workspace = new ConnectorWorkspace("workspace-1", "Test", root, "fingerprint");
            var resolver = new LocalProjectPathResolver(new WorkspaceContext(workspace));
            Service = new WindowsProjectCodeNavigationService(resolver);
        }

        public string Root { get; }
        public string LogicalRoot { get; }
        public WindowsProjectCodeNavigationService Service { get; }

        public static TestContext Create()
        {
            var root = Path.Combine(Path.GetTempPath(), $"chatos-navigation-{Guid.NewGuid():N}");
            Directory.CreateDirectory(root);
            return new TestContext(root);
        }

        public void Dispose()
        {
            if (Directory.Exists(Root)) Directory.Delete(Root, recursive: true);
        }
    }

    private sealed class WorkspaceContext(ConnectorWorkspace workspace) : IConnectorWorkspaceContext
    {
        public string? DeviceId => "device-1";
        public IReadOnlyList<ConnectorWorkspace> Workspaces => [workspace];
        public ConnectorWorkspace? Find(string workspaceId) => workspaceId == workspace.Id ? workspace : null;
    }
}
