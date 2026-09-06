using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using ChatOS.Connector.Approval;
using ChatOS.Connector.Plugins;
using ChatOS.Connector.Relay;
using ChatOS.Connector.Runtime;
using ChatOS.Connector.Workspaces;

namespace ChatOS.Connector.Tests;

public sealed class PluginRelayHandlerTests : IDisposable
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(),
        $"chatos-plugin-relay-{Guid.NewGuid():N}");

    [Fact]
    public async Task PreparesExecutesAndCancelsInstalledPluginSession()
    {
        var workspace = Path.Combine(_directory, "workspace");
        var installation = CreateInstallation();
        Directory.CreateDirectory(workspace);
        var record = Record(installation);
        var runtime = Runtime(workspace);
        await runtime.InitializeAsync();
        var approvalStore = new ApprovalStore();
        var approvals = new CommandApprovalCoordinator(approvalStore);
        await approvals.SetModeAsync(ConnectorApprovalMode.FullControl, fullControlRiskConfirmed: true);
        var client = new FakeMcpClient();
        var handler = new PluginRelayHandler(
            new InstalledStore(record),
            new PluginManagement(),
            new PluginManifestLoader(Path.Combine(_directory, "runtime")),
            new ClientFactory(client),
            new PluginRuntimeSessionStore(),
            runtime,
            new LocalProjectPathResolver(runtime),
            approvals);

        var prepared = await handler.HandleAsync(Request(
            "plugin_prepare_request",
            "request-1",
            "workspace-1",
            new
            {
                run_id = "run-1",
                plugin_id = "plugin-1",
                release_id = "release-1",
                artifact_sha256 = new string('a', 64),
                component_key = "main",
                project_id = "project-1",
                permission_snapshot = new[] { "process.spawn", "workspace.read" },
                tool_allowlist = new[] { "echo" },
                tool_blocklist = Array.Empty<string>(),
            }), CancellationToken.None);

        Assert.Equal(200, prepared.Status);
        var adapterSessionId = prepared.Body.GetProperty("adapter_session_id").GetString()!;
        Assert.Equal("echo", prepared.Body.GetProperty("mcp").GetProperty("tools")[0]
            .GetProperty("name").GetString());
        Assert.True(client.Started);

        await Assert.ThrowsAsync<RelayRequestException>(() => handler.HandleAsync(Request(
            "plugin_execute_request",
            "request-mismatched-project",
            "workspace-1",
            new
            {
                plugin_id = "plugin-1",
                release_id = "release-1",
                artifact_sha256 = new string('a', 64),
                component_key = "main",
                adapter_session_id = adapterSessionId,
                invocation_id = "invocation-mismatched-project",
                operation = "mcp_tools_call",
                tool_name = "echo",
                arguments = new { value = "wrong" },
                project_id = "project-2",
            }), CancellationToken.None));

        var executed = await handler.HandleAsync(Request(
            "plugin_execute_request",
            "request-2",
            "workspace-1",
            new
            {
                plugin_id = "plugin-1",
                release_id = "release-1",
                artifact_sha256 = new string('a', 64),
                component_key = "main",
                adapter_session_id = adapterSessionId,
                invocation_id = "invocation-1",
                operation = "mcp_tools_call",
                tool_name = "echo",
                arguments = new { value = "hello" },
                project_id = "project-1",
            }), CancellationToken.None);

        Assert.Equal("hello", executed.Body.GetProperty("result").GetProperty("echo").GetString());
        Assert.Equal("echo", client.LastToolName);

        var cancelled = await handler.HandleAsync(Request(
            "plugin_cancel_request",
            "request-3",
            "workspace-1",
            new
            {
                run_id = "run-1",
                adapter_session_id = adapterSessionId,
                project_id = "project-1",
            }), CancellationToken.None);

        Assert.Equal("cancelled", cancelled.Body.GetProperty("status").GetString());
        Assert.True(client.Terminated);
    }

    [Fact]
    public async Task SkillV2SeparatesCatalogActivationAndResourceReads()
    {
        var installation = Path.Combine(_directory, "installed-skill");
        var skillDirectory = Path.Combine(installation, "skills", "fixture-skill");
        var references = Path.Combine(skillDirectory, "references");
        Directory.CreateDirectory(references);
        File.WriteAllText(Path.Combine(installation, "chatos.plugin.json"),
            """{"schemaVersion":3,"name":"fixture","version":"1.0.0","skills":["./skills/fixture-skill"],"mcpServers":{}}""");
        File.WriteAllText(Path.Combine(skillDirectory, "SKILL.md"), """
            ---
            name: fixture-skill
            description: Fixture Skill instructions.
            ---
            # Fixture Skill
            Read the guide only when needed.
            """);
        File.WriteAllText(Path.Combine(references, "guide.md"), "# Guide\nUse fresh screenshots.\n");
        var record = Record(installation);
        var runtime = Runtime(Path.Combine(_directory, "workspace-skill"));
        Directory.CreateDirectory(Path.Combine(_directory, "workspace-skill"));
        await runtime.InitializeAsync();
        var sessions = new PluginRuntimeSessionStore();
        var handler = new PluginRelayHandler(
            new InstalledStore(record),
            new PluginManagement(),
            new PluginManifestLoader(Path.Combine(_directory, "runtime-skill")),
            new ClientFactory(new FakeMcpClient()),
            sessions,
            runtime,
            new LocalProjectPathResolver(runtime),
            new CommandApprovalCoordinator(new ApprovalStore()));

        var instructions = File.ReadAllBytes(Path.Combine(skillDirectory, "SKILL.md"));
        var guide = File.ReadAllBytes(Path.Combine(references, "guide.md"));
        var resource = JsonSerializer.SerializeToElement(new
        {
            relative_path = "references/guide.md",
            kind = "reference",
            size_bytes = guide.LongLength,
            sha256 = Sha256(guide),
        });
        var resources = JsonSerializer.SerializeToElement(new[] { resource });
        var metadata = JsonSerializer.SerializeToElement(new
        {
            name = "fixture-skill",
            description = "Fixture Skill instructions.",
            role = "leaf",
            activation_policy = "model_or_user",
            context_mode = "inline",
            required_skills = Array.Empty<string>(),
            related_skills = Array.Empty<string>(),
            extra = new { },
        });
        var resourceManifestSha256 = CanonicalSha256(resources);
        var snapshotPayload = JsonSerializer.SerializeToElement(new Dictionary<string, object?>
        {
            ["protocol_version"] = 2,
            ["skill_id"] = "fixture-skill",
            ["relative_skill_path"] = "skills/fixture-skill/SKILL.md",
            ["metadata"] = metadata,
            ["instructions_sha256"] = Sha256(instructions),
            ["resource_manifest_sha256"] = resourceManifestSha256,
        });
        var expectedSnapshot = JsonSerializer.SerializeToElement(new Dictionary<string, object?>
        {
            ["protocol_version"] = 2,
            ["skill_id"] = "fixture-skill",
            ["relative_skill_path"] = "skills/fixture-skill/SKILL.md",
            ["metadata"] = metadata,
            ["instructions_sha256"] = Sha256(instructions),
            ["resource_manifest_sha256"] = resourceManifestSha256,
            ["resources"] = new[] { resource },
            ["snapshot_sha256"] = CanonicalSha256(snapshotPayload),
        });

        var prepared = await handler.HandleAsync(Request(
            "plugin_prepare_request", "prepare-skill", string.Empty, new
            {
                run_id = "run-skill",
                plugin_id = "plugin-1",
                release_id = "release-1",
                artifact_sha256 = new string('a', 64),
                component_key = "fixture-skill",
                permission_snapshot = Array.Empty<string>(),
                skill_runtime_protocol = 2,
                skill_keys = new[] { "fixture-skill" },
                skill_snapshot = expectedSnapshot,
            }), CancellationToken.None);
        var adapterSessionId = prepared.Body.GetProperty("adapter_session_id").GetString()!;
        Assert.False(prepared.Body.GetProperty("skills")[0].TryGetProperty("instructions", out _));

        var activated = await handler.HandleAsync(Request(
            "plugin_execute_request", "activate-skill", string.Empty, new
            {
                plugin_id = "plugin-1",
                release_id = "release-1",
                artifact_sha256 = new string('a', 64),
                component_key = "fixture-skill",
                adapter_session_id = adapterSessionId,
                invocation_id = "activation-1",
                operation = "skill_activate",
            }), CancellationToken.None);
        Assert.Contains("# Fixture Skill",
            activated.Body.GetProperty("result").GetProperty("instructions").GetString());

        var resourceRead = await handler.HandleAsync(Request(
            "plugin_execute_request", "read-skill", string.Empty, new
            {
                plugin_id = "plugin-1",
                release_id = "release-1",
                artifact_sha256 = new string('a', 64),
                component_key = "fixture-skill",
                adapter_session_id = adapterSessionId,
                invocation_id = "read-1",
                operation = "skill_read_resource",
                arguments = new { relative_path = "references/guide.md", offset = 0, max_chars = 8 },
            }), CancellationToken.None);
        Assert.Equal("# Guide\n",
            resourceRead.Body.GetProperty("result").GetProperty("content").GetString());
    }

    [Fact]
    public async Task RejectsDeviceScopeWorkspacePermissionAndMismatchedIdentity()
    {
        var installation = CreateInstallation();
        var runtime = Runtime(Path.Combine(_directory, "workspace"));
        Directory.CreateDirectory(Path.Combine(_directory, "workspace"));
        await runtime.InitializeAsync();
        var handler = new PluginRelayHandler(
            new InstalledStore(Record(installation)),
            new PluginManagement(),
            new PluginManifestLoader(Path.Combine(_directory, "runtime")),
            new ClientFactory(new FakeMcpClient()),
            new PluginRuntimeSessionStore(),
            runtime,
            new LocalProjectPathResolver(runtime),
            new CommandApprovalCoordinator(new ApprovalStore()));

        var error = await Assert.ThrowsAsync<RelayRequestException>(() => handler.HandleAsync(Request(
            "plugin_prepare_request",
            "request-1",
            string.Empty,
            new
            {
                run_id = "run-1",
                plugin_id = "plugin-1",
                release_id = "release-1",
                artifact_sha256 = new string('a', 64),
                component_key = "main",
                permission_snapshot = new[] { "process.spawn", "workspace.read" },
            }), CancellationToken.None));

        Assert.Equal(403, error.StatusCode);
    }

    [Fact]
    public async Task PreparesHttpMcpThroughThePluginRuntimeLifecycle()
    {
        var installation = CreateHttpInstallation();
        var record = new InstalledPluginRecord(
            "plugin-1",
            "release-1",
            "1.0.0",
            new string('a', 64),
            installation,
            DateTimeOffset.UtcNow,
            ["network.domain:127.0.0.1"]);
        var runtime = Runtime(Path.Combine(_directory, "workspace-http"));
        Directory.CreateDirectory(Path.Combine(_directory, "workspace-http"));
        await runtime.InitializeAsync();
        var client = new FakeMcpClient();
        var clientFactory = new ClientFactory(client);
        var handler = new PluginRelayHandler(
            new InstalledStore(record),
            new PluginManagement(),
            new PluginManifestLoader(Path.Combine(_directory, "runtime-http")),
            clientFactory,
            new PluginRuntimeSessionStore(),
            runtime,
            new LocalProjectPathResolver(runtime),
            new CommandApprovalCoordinator(new ApprovalStore()));

        var prepared = await handler.HandleAsync(Request(
            "plugin_prepare_request",
            "request-http",
            string.Empty,
            new
            {
                run_id = "run-http",
                plugin_id = "plugin-1",
                release_id = "release-1",
                artifact_sha256 = new string('a', 64),
                component_key = "remote",
                permission_snapshot = new[] { "network.domain:127.0.0.1" },
            }), CancellationToken.None);

        Assert.Equal(200, prepared.Status);
        Assert.Equal("http", prepared.Body.GetProperty("mcp").GetProperty("transport").GetString());
        Assert.Equal(JsonValueKind.Null,
            prepared.Body.GetProperty("mcp").GetProperty("oauth_connection_id").ValueKind);
        Assert.Equal("http", clientFactory.LastLaunch?.Transport);
        Assert.Equal("http://127.0.0.1:43111/mcp", clientFactory.LastLaunch?.HttpEndpoint?.AbsoluteUri);
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
            """{"schemaVersion":3,"name":"test-plugin","version":"1.0.0","mcpServers":{"main":{"type":"stdio","bin":"test-plugin"}},"permissions":[{"permission":"process.spawn","required":true,"components":["main"]},{"permission":"workspace.read","required":true,"components":["main"]}]}""");
        File.WriteAllText(Path.Combine(installation, "bin", "test-plugin"), "native executable");
        return installation;
    }

    private string CreateHttpInstallation()
    {
        var installation = Path.Combine(_directory, "installed-http");
        Directory.CreateDirectory(installation);
        File.WriteAllText(
            Path.Combine(installation, "chatos.plugin.json"),
            """{"schemaVersion":3,"name":"test-plugin","version":"1.0.0","mcpServers":{"remote":{"type":"http","url":"http://127.0.0.1:43111/mcp"}},"permissions":[{"permission":"network.domain:127.0.0.1","required":true,"components":["remote"]}]}""");
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

    private static ConnectorRuntimeContext Runtime(string workspace) => new(
        new RuntimeStore(new ConnectorPersistentState(
            new Uri("https://gateway.example"),
            new ConnectorUser("owner-1", "owner", "Owner", "user"),
            "device-1",
            "Windows PC",
            [new ConnectorWorkspace("workspace-1", "Project", workspace, "fingerprint")],
            new RemoteControlTrust(false, 120, new Dictionary<string, string>()))),
        new TokenStore());

    private static RelayRequest Request(string type, string requestId, string workspaceId, object body) => new()
    {
        Type = type,
        RequestId = requestId,
        OwnerUserId = "owner-1",
        DeviceId = "device-1",
        WorkspaceId = workspaceId,
        Body = JsonSerializer.SerializeToElement(body),
    };

    private static string CanonicalSha256(JsonElement value) =>
        Sha256(Encoding.UTF8.GetBytes(CanonicalJson.Serialize(value)));

    private static string Sha256(byte[] value) =>
        Convert.ToHexString(SHA256.HashData(value)).ToLowerInvariant();

    private sealed class FakeMcpClient : IPluginMcpClient
    {
        public bool Started { get; private set; }
        public bool Terminated { get; private set; }
        public string? LastToolName { get; private set; }

        public Task StartAsync(CancellationToken cancellationToken = default)
        {
            Started = true;
            return Task.CompletedTask;
        }

        public Task<PluginMcpInitialization> InitializeAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult(new PluginMcpInitialization(
                "Use safely",
                [JsonSerializer.SerializeToElement(new
                {
                    name = "echo",
                    inputSchema = new { type = "object" },
                    _meta = new Dictionary<string, object>
                    {
                        ["chatos/requiredPermissions"] = new[] { "workspace.read" },
                        ["chatos/timeoutMs"] = 1_000,
                    },
                })]));

        public Task<JsonElement> CallToolAsync(
            string name,
            JsonElement arguments,
            TimeSpan timeout,
            CancellationToken cancellationToken = default)
        {
            LastToolName = name;
            return Task.FromResult(JsonSerializer.SerializeToElement(new
            {
                echo = arguments.GetProperty("value").GetString(),
            }));
        }

        public Task TerminateAsync()
        {
            Terminated = true;
            return Task.CompletedTask;
        }

        public ValueTask DisposeAsync() => ValueTask.CompletedTask;
    }

    private sealed class ClientFactory(FakeMcpClient client) : IPluginMcpClientFactory
    {
        public PreparedPluginLaunch? LastLaunch { get; private set; }
        public IPluginMcpClient Create(PreparedPluginLaunch launch)
        {
            LastLaunch = launch;
            return client;
        }
    }

    private sealed class InstalledStore(InstalledPluginRecord record) : IInstalledPluginStore
    {
        public Task<IReadOnlyList<InstalledPluginRecord>> ListAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<InstalledPluginRecord>>([record]);
        public Task<InstalledPluginRecord?> GetAsync(string pluginId, CancellationToken cancellationToken = default) =>
            Task.FromResult<InstalledPluginRecord?>(pluginId == record.PluginId ? record : null);
        public Task SaveAsync(InstalledPluginRecord value, CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task DeleteAsync(string pluginId, CancellationToken cancellationToken = default) => Task.CompletedTask;
    }

    private sealed class PluginManagement : ILocalPluginManagementService
    {
        public Task<IReadOnlyList<LocalConnectorPlugin>> ListAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<LocalConnectorPlugin>>([
                new("plugin-1", "Test", string.Empty, "Tools", "ChatOS", "1.0.0", true, false, true, true, ["process.spawn"]),
            ]);
        public Task<InstalledPluginRecord> InstallAsync(string pluginId, CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();
        public Task UninstallAsync(string pluginId, CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();
        public Task SetEnabledAsync(string pluginId, bool enabled, CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();
    }

    private sealed class RuntimeStore(ConnectorPersistentState state) : IConnectorPersistentStateStore
    {
        public Task<ConnectorPersistentState?> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult<ConnectorPersistentState?>(state);
        public Task SaveAsync(ConnectorPersistentState? value, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;
    }

    private sealed class TokenStore : IConnectorAccessTokenStore
    {
        public ValueTask<string?> GetAccessTokenAsync(CancellationToken cancellationToken = default) =>
            ValueTask.FromResult<string?>("token");
        public ValueTask SetAccessTokenAsync(string token, CancellationToken cancellationToken = default) =>
            ValueTask.CompletedTask;
        public ValueTask ClearAsync(CancellationToken cancellationToken = default) => ValueTask.CompletedTask;
    }

    private sealed class ApprovalStore : IConnectorApprovalStore
    {
        private ConnectorApprovalMode? _mode;
        public Task<ConnectorApprovalMode?> ReadModeAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult(_mode);
        public Task SaveModeAsync(ConnectorApprovalMode mode, CancellationToken cancellationToken = default)
        {
            _mode = mode;
            return Task.CompletedTask;
        }
        public Task AppendAsync(ConnectorApprovalHistoryEntry entry, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;
        public Task<IReadOnlyList<ConnectorApprovalHistoryEntry>> ReadHistoryAsync(
            int limit = 1000,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<ConnectorApprovalHistoryEntry>>([]);
    }
}
