using System.Text.Json;
using ChatOS.Connector.Plugins;
using ChatOS.Connector.Persistence;
using ChatOS.Connector.Security;

namespace ChatOS.Connector.Tests;

public sealed class PluginManifestLoaderTests : IDisposable
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(),
        $"chatos-plugin-manifest-{Guid.NewGuid():N}");

    [Fact]
    public async Task ResolvesVerifiedNpmBinAndCreatesIsolatedRuntimeDirectories()
    {
        var installation = CreateInstallation();
        var runtime = Path.Combine(_directory, "runtime");
        var loader = new PluginManifestLoader(runtime);

        var launch = await loader.PrepareAsync(
            Record(installation),
            "main",
            null,
            "session-1",
            _directory,
            new HashSet<string>(StringComparer.Ordinal) { "process.spawn", "workspace.read" },
            "owner-1",
            "device-1");

        Assert.Equal(Path.Combine(installation, "bin", "test-plugin"), launch.ExecutablePath);
        Assert.Equal("main", launch.ComponentKey);
        Assert.Equal(_directory, launch.Environment["CHATOS_WORKSPACE"]);
        Assert.True(Directory.Exists(launch.ArtifactPath));
        using var host = JsonDocument.Parse(await File.ReadAllTextAsync(
            Path.Combine(launch.VisualSessionPath, "host.json")));
        Assert.Equal("session-1", host.RootElement.GetProperty("adapter_session_id").GetString());
    }

    [Fact]
    public async Task AllPluginsUseDifferentDataDirectoriesForDifferentChatOsUsers()
    {
        var installation = CreateInstallation();
        var loader = new PluginManifestLoader(Path.Combine(_directory, "runtime-users"));
        var permissions = new HashSet<string>(StringComparer.Ordinal)
        {
            "process.spawn",
            "workspace.read",
        };

        var first = await loader.PrepareAsync(
            Record(installation),
            "main",
            null,
            "session-user-1",
            null,
            permissions,
            "owner-1",
            "device-1");
        var second = await loader.PrepareAsync(
            Record(installation),
            "main",
            null,
            "session-user-2",
            null,
            permissions,
            "owner-2",
            "device-1");

        Assert.NotEqual(
            first.Environment["CHATOS_PLUGIN_DATA_DIR"],
            second.Environment["CHATOS_PLUGIN_DATA_DIR"]);
        Assert.NotEqual(
            first.Environment["CHATOS_PLUGIN_CACHE_DIR"],
            second.Environment["CHATOS_PLUGIN_CACHE_DIR"]);
        Assert.Contains(
            Path.Combine("data", "users"),
            first.Environment["CHATOS_PLUGIN_DATA_DIR"],
            StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public async Task ProjectRuntimeContextSeparatesProjectsAndFallsBackToDevicePublicProject()
    {
        var installation = CreateInstallation();
        await File.WriteAllTextAsync(
            Path.Combine(installation, "chatos.plugin.json"),
            """{"schemaVersion":3,"name":"test-plugin","version":"1.0.0","mcpServers":{"main":{"type":"stdio","bin":"test-plugin","args":["serve"]}},"permissions":[{"permission":"process.spawn","required":true,"components":["main"]}],"runtimeContext":{"scope":"project","components":["main"],"optional":["project.id","workspace.id","workspace.root"],"storageIsolation":"project","missingContext":"device"}}""");
        var loader = new PluginManifestLoader(Path.Combine(_directory, "runtime-projects"));
        var permissions = new HashSet<string>(StringComparer.Ordinal) { "process.spawn" };

        var first = await loader.PrepareAsync(
            Record(installation),
            "main",
            null,
            "session-project-1",
            null,
            permissions,
            "owner-1",
            "device-1",
            workspaceId: "workspace-1",
            projectId: "project-1");
        var second = await loader.PrepareAsync(
            Record(installation),
            "main",
            null,
            "session-project-2",
            null,
            permissions,
            "owner-1",
            "device-1",
            workspaceId: "workspace-1",
            projectId: "project-2");
        var publicProject = await loader.PrepareAsync(
            Record(installation),
            "main",
            null,
            "session-public-project",
            null,
            permissions,
            "owner-1",
            "device-1");

        Assert.NotEqual(
            first.Environment["CHATOS_PLUGIN_DATA_DIR"],
            second.Environment["CHATOS_PLUGIN_DATA_DIR"]);
        Assert.NotEqual(
            first.Environment["CHATOS_PLUGIN_DATA_DIR"],
            publicProject.Environment["CHATOS_PLUGIN_DATA_DIR"]);
        Assert.Equal("project", first.Environment["CHATOS_CONTEXT_SCOPE"]);
        Assert.Equal("project-1", first.Environment["CHATOS_PROJECT_ID"]);
        Assert.Equal("device", publicProject.Environment["CHATOS_CONTEXT_SCOPE"]);
        Assert.False(publicProject.Environment.ContainsKey("CHATOS_PROJECT_ID"));
    }

    [Fact]
    public async Task RejectsMissingRequiredPermissionAndEscapingBin()
    {
        var installation = CreateInstallation();
        var loader = new PluginManifestLoader(Path.Combine(_directory, "runtime"));
        await Assert.ThrowsAsync<PluginRuntimeException>(() => loader.PrepareAsync(
            Record(installation),
            "main",
            null,
            "session-1",
            null,
            new HashSet<string>(),
            "owner-1",
            "device-1"));

        await File.WriteAllTextAsync(
            Path.Combine(installation, "package.json"),
            """{"name":"test-plugin","bin":{"test-plugin":"../outside.exe"}}""");
        await Assert.ThrowsAsync<PluginRuntimeException>(() => loader.PrepareAsync(
            Record(installation),
            "main",
            null,
            "session-2",
            null,
            new HashSet<string> { "process.spawn", "workspace.read" },
            "owner-1",
            "device-1"));
    }

    [Fact]
    public async Task ResolvesExactCredentialTemplateWithoutWritingSecretToRuntimeMetadata()
    {
        var installation = CreateInstallation();
        await File.WriteAllTextAsync(
            Path.Combine(installation, "chatos.plugin.json"),
            """{"schemaVersion":3,"name":"test-plugin","version":"1.0.0","mcpServers":{"main":{"type":"stdio","bin":"test-plugin","env":{"API_TOKEN":"${credential:api.token}"}}},"permissions":[{"permission":"process.spawn","required":true,"components":["main"]},{"permission":"credential.use","required":true,"components":["main"]}]}""");
        var database = new LocalStateDatabase(Path.Combine(_directory, "credential-state.db"));
        await database.InitializeAsync();
        var vault = new PluginCredentialVault(
            new MemorySecrets(),
            new SqlitePluginCredentialMetadataStore(database));
        await vault.UpsertAsync(new PluginCredentialScope(
            "owner-1", "device-1", "plugin-1", "release-1", "main", "api.token"),
            "top-secret-token");
        var loader = new PluginManifestLoader(Path.Combine(_directory, "runtime-secret"), vault);

        var launch = await loader.PrepareAsync(
            Record(installation),
            "main",
            null,
            "session-secret",
            null,
            new HashSet<string> { "process.spawn", "credential.use" },
            "owner-1",
            "device-1");

        Assert.Equal("top-secret-token", launch.Environment["API_TOKEN"]);
        Assert.DoesNotContain(
            "top-secret-token",
            await File.ReadAllTextAsync(Path.Combine(launch.VisualSessionPath, "host.json")),
            StringComparison.Ordinal);
    }

    [Fact]
    public async Task PreparesHttpTransportWithBoundCredentialSnapshotAndNoWorkspaceAccess()
    {
        var installation = CreateInstallation();
        await File.WriteAllTextAsync(
            Path.Combine(installation, "chatos.plugin.json"),
            """{"schemaVersion":3,"name":"test-plugin","version":"1.0.0","mcpServers":{"remote":{"type":"http","url":"http://127.0.0.1:43111/mcp","headers":{"Authorization":"Bearer ${credential:api.token}","X-Plugin-Client":"chatos"},"connectTimeoutMs":5000}},"permissions":[{"permission":"network.domain:127.0.0.1","required":true,"components":["remote"]},{"permission":"credential.use:demo","required":true,"components":["remote"]}]}""");
        var database = new LocalStateDatabase(Path.Combine(_directory, "http-credential-state.db"));
        await database.InitializeAsync();
        var vault = new PluginCredentialVault(
            new MemorySecrets(),
            new SqlitePluginCredentialMetadataStore(database));
        await vault.UpsertAsync(new PluginCredentialScope(
            "owner-1", "device-1", "plugin-1", "release-1", "remote", "api.token"),
            "top-secret-token");
        var loader = new PluginManifestLoader(Path.Combine(_directory, "runtime-http"), vault);

        var launch = await loader.PrepareAsync(
            Record(installation),
            "remote",
            null,
            "session-http",
            null,
            new HashSet<string> { "network.domain:127.0.0.1", "credential.use:demo" },
            "owner-1",
            "device-1");

        Assert.Equal("http", launch.Transport);
        Assert.Equal("http://127.0.0.1:43111/mcp", launch.HttpEndpoint?.AbsoluteUri);
        Assert.NotNull(launch.CredentialBinding);
        Assert.Equal(64, launch.CredentialBinding!.SnapshotSha256.Length);
        Assert.DoesNotContain("top-secret-token", JsonSerializer.Serialize(launch.HttpHeaderTemplates));
        await Assert.ThrowsAsync<PluginRuntimeException>(() => loader.PrepareAsync(
            Record(installation),
            "remote",
            null,
            "session-http-workspace",
            _directory,
            new HashSet<string> { "network.domain:127.0.0.1", "credential.use:demo" },
            "owner-1",
            "device-1"));
    }

    public void Dispose()
    {
        try
        {
            if (Directory.Exists(_directory))
            {
                Directory.Delete(_directory, recursive: true);
            }
        }
        catch (IOException)
        {
        }
    }

    private string CreateInstallation()
    {
        var installation = Path.Combine(_directory, "installed");
        Directory.CreateDirectory(Path.Combine(installation, "bin"));
        File.WriteAllText(
            Path.Combine(installation, "package.json"),
            """{"name":"test-plugin","version":"1.0.0","bin":{"test-plugin":"bin/test-plugin"}}""");
        File.WriteAllText(
            Path.Combine(installation, "chatos.plugin.json"),
            """{"schemaVersion":3,"name":"test-plugin","version":"1.0.0","interface":{"displayName":"Test Plugin"},"mcpServers":{"main":{"type":"stdio","bin":"test-plugin","args":["serve"]}},"permissions":[{"permission":"process.spawn","required":true,"components":["main"]},{"permission":"workspace.read","required":true,"components":["main"]}]}""");
        File.WriteAllText(Path.Combine(installation, "bin", "test-plugin"), "native executable");
        return installation;
    }

    private static InstalledPluginRecord Record(string installation) => new(
        "plugin-1",
        "release-1",
        "1.0.0",
        new string('a', 64),
        installation,
        DateTimeOffset.UtcNow,
        ["process.spawn", "workspace.read"]);

    private sealed class MemorySecrets : IConnectorSecretStore
    {
        private readonly Dictionary<string, string> _values = new(StringComparer.Ordinal);
        public ValueTask<string?> GetAsync(string key, CancellationToken cancellationToken = default) =>
            ValueTask.FromResult(_values.GetValueOrDefault(key));
        public ValueTask SetAsync(string key, string value, CancellationToken cancellationToken = default)
        {
            _values[key] = value;
            return ValueTask.CompletedTask;
        }
        public ValueTask DeleteAsync(string key, CancellationToken cancellationToken = default)
        {
            _values.Remove(key);
            return ValueTask.CompletedTask;
        }
    }
}
