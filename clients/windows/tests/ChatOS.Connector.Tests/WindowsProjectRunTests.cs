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

        var npm = Assert.Single(analysis.Targets, value => value.Id == "npm:dev");
        Assert.Equal("npm run dev", npm.Command);
        Assert.Contains(analysis.Targets, value => value.Id == "cargo:test");
        Assert.DoesNotContain(analysis.Targets, value => value.Id == "cargo:run");
        Assert.DoesNotContain(analysis.Targets, value => value.Id.Contains("calc", StringComparison.Ordinal));
        Assert.Contains(analysis.ConfigurationFiles, value => value.Path == "package.json");
    }

    [Fact]
    public void AnalyzerOnlyPublishesCargoRunForRunnableNestedPackages()
    {
        var nested = Path.Combine(_directory, "apps", "demo");
        Directory.CreateDirectory(Path.Combine(nested, "src"));
        File.WriteAllText(Path.Combine(nested, "Cargo.toml"), "[package]\nname='demo'");
        File.WriteAllText(Path.Combine(nested, "src", "main.rs"), "fn main() {}");

        var analysis = WindowsProjectRunAnalyzer.Analyze(_directory);

        var run = Assert.Single(analysis.Targets, value => value.Command == "cargo run");
        Assert.Equal(nested, run.WorkingDirectory);
        Assert.Equal("src/main.rs", run.Entrypoint);
        Assert.Contains("apps/demo", run.Id, StringComparison.Ordinal);
    }

    [Fact]
    public void AnalyzerDiscoversNestedDotNetProjectsThroughAccessibleDirectories()
    {
        var nested = Path.Combine(_directory, "src");
        Directory.CreateDirectory(nested);
        File.WriteAllText(Path.Combine(nested, "Sample.csproj"), "<Project Sdk=\"Microsoft.NET.Sdk\" />");

        var analysis = WindowsProjectRunAnalyzer.Analyze(_directory);

        Assert.Contains(analysis.Targets, value => value.Id == "dotnet:src/Sample.csproj");
        Assert.Contains(analysis.ConfigurationFiles, value => value.Path == "src/Sample.csproj");
    }

    [Theory]
    [InlineData(true, -1, "stopped")]
    [InlineData(true, 0, "stopped")]
    [InlineData(false, 0, "exited")]
    [InlineData(false, 7, "failed")]
    public void ProcessFinalStatusDistinguishesUserStopFromFailure(
        bool stopRequested,
        int exitCode,
        string expected)
    {
        Assert.Equal(expected, WindowsProjectRunProcess.FinalStatus(stopRequested, exitCode));
    }

    [Fact]
    public async Task WindowsProcessJobTerminatesDescendantProcess()
    {
        if (!OperatingSystem.IsWindows()) return;

        var childPidPath = Path.Combine(_directory, "child.pid");
        var escapedPath = childPidPath.Replace("'", "''", StringComparison.Ordinal);
        using var job = new WindowsProcessJob();
        using var parent = new System.Diagnostics.Process
        {
            StartInfo = new System.Diagnostics.ProcessStartInfo("powershell.exe")
            {
                UseShellExecute = false,
                CreateNoWindow = true,
            },
        };
        parent.StartInfo.ArgumentList.Add("-NoProfile");
        parent.StartInfo.ArgumentList.Add("-Command");
        parent.StartInfo.ArgumentList.Add(
            $"Start-Sleep -Milliseconds 500; " +
            "$start = New-Object System.Diagnostics.ProcessStartInfo; " +
            "$start.FileName = 'ping.exe'; " +
            "$start.Arguments = '127.0.0.1 -t'; " +
            "$start.UseShellExecute = $false; " +
            "$start.CreateNoWindow = $true; " +
            "$child = [System.Diagnostics.Process]::Start($start); " +
            $"Set-Content -LiteralPath '{escapedPath}' -Value $child.Id; " +
            "Wait-Process -Id $child.Id");

        Assert.True(parent.Start());
        job.Assign(parent);
        var childPid = await ReadProcessIdWhenClosedAsync(childPidPath);
        Assert.True(IsProcessRunning(childPid));

        job.Terminate();

        Assert.True(parent.WaitForExit(5_000));
        Assert.True(SpinWait.SpinUntil(() => !IsProcessRunning(childPid), 5_000));
    }

    private static async Task<int> ReadProcessIdWhenClosedAsync(string path)
    {
        for (var attempt = 0; attempt < 50; attempt++)
        {
            try
            {
                if (File.Exists(path))
                {
                    var value = await File.ReadAllTextAsync(path);
                    if (int.TryParse(value, out var processId)) return processId;
                }
            }
            catch (IOException)
            {
                // PowerShell creates the file before Set-Content closes its
                // exclusive writer. File existence alone is not readiness.
            }
            await Task.Delay(100);
        }

        throw new TimeoutException("The child process id file did not become readable.");
    }

    private static bool IsProcessRunning(int processId)
    {
        try
        {
            using var process = System.Diagnostics.Process.GetProcessById(processId);
            return !process.HasExited;
        }
        catch (ArgumentException)
        {
            return false;
        }
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
