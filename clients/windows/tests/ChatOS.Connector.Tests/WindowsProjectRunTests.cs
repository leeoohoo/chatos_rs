using ChatOS.Connector.Persistence;
using ChatOS.Connector.Relay;
using ChatOS.Connector.Runtime;
using ChatOS.Connector.Workspaces;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.Tests;

public sealed class WindowsProjectRunTests : IAsyncLifetime
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(),
        $"chatos-project-run-{Guid.NewGuid():N}");
    private LocalStateDatabase _database = null!;

    public async Task InitializeAsync()
    {
        Directory.CreateDirectory(_directory);
        _database = new LocalStateDatabase(Path.Combine(_directory, "state.db"));
        await _database.InitializeAsync();
    }

    public Task DisposeAsync()
    {
        Directory.Delete(_directory, recursive: true);
        return Task.CompletedTask;
    }

    [Fact]
    public void AnalyzerDiscoversLocalTargetsAndIgnoresUnsafeScriptNames()
    {
        File.WriteAllText(Path.Combine(_directory, "package.json"), """
            {
              "scripts": {
                "dev": "vite",
                "bad\" & calc": "ignored"
              }
            }
            """);
        File.WriteAllText(Path.Combine(_directory, "Cargo.toml"), "[package]\nname='demo'");

        var analysis = WindowsProjectRunAnalyzer.Analyze(_directory);

        Assert.Contains(analysis.Targets, value => value.Id == "npm:dev");
        Assert.Contains(analysis.Targets, value => value.Id == "cargo:run");
        Assert.DoesNotContain(analysis.Targets, value => value.Id.Contains("calc", StringComparison.Ordinal));
        Assert.Contains(analysis.ConfigurationFiles, value => value.Path == "package.json");
    }

    [Fact]
    public async Task ServiceResolvesTheAccountProjectAndPersistsItsDefaultTargetLocally()
    {
        File.WriteAllText(Path.Combine(_directory, "package.json"), """
            { "scripts": { "dev": "node server.js", "test": "node test.js" } }
            """);
        var registry = new SqliteProjectRegistry(_database);
        var project = await registry.CreateAsync("user-1", new("Local", "workspace-1"));
        var stateStore = new SqliteConnectorPersistentStateStore(_database);
        await stateStore.SaveAsync(new ConnectorPersistentState(
            new Uri("https://gateway.example"),
            new ConnectorUser("user-1", "user", "User", "user"),
            "device-1",
            "Windows PC",
            [new ConnectorWorkspace("workspace-1", "Workspace", _directory, "fingerprint")],
            new RemoteControlTrust(false, 120, new Dictionary<string, string>())));
        var runtime = new ConnectorRuntimeContext(stateStore, new EmptyTokenStore());
        var settings = new WindowsProjectRunSettingsStore(_database);
        using (var service = new WindowsProjectRunService(registry, runtime, settings))
        {
            var analyzed = await service.AnalyzeAsync(project.Id);
            Assert.Equal(2, analyzed.Targets.Count);
            _ = await service.SetDefaultTargetAsync(project.Id, "npm:test");
        }

        using var restored = new WindowsProjectRunService(registry, runtime, settings);
        var catalog = await restored.FetchCatalogAsync(project.Id);

        Assert.Equal("npm:test", catalog.DefaultTargetId);
        Assert.True(catalog.Targets.Single(value => value.Id == "npm:test").IsDefault);
        Assert.Contains((await restored.FetchEnvironmentAsync(project.Id)).ConfigurationFiles,
            value => value.Path == "package.json");
    }

    private sealed class EmptyTokenStore : IConnectorAccessTokenStore
    {
        public ValueTask<string?> GetAccessTokenAsync(
            CancellationToken cancellationToken = default) => ValueTask.FromResult<string?>(null);

        public ValueTask SetAccessTokenAsync(
            string token,
            CancellationToken cancellationToken = default) => ValueTask.CompletedTask;

        public ValueTask ClearAsync(
            CancellationToken cancellationToken = default) => ValueTask.CompletedTask;
    }
}
