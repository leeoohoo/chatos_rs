using System.Diagnostics;
using System.Text.Json;
using ChatOS.Connector.Plugins;
using ChatOS.Connector.Sandbox;
using ChatOS.Connector.Terminal;
using Microsoft.Win32.SafeHandles;

namespace ChatOS.Connector.Tests;

public sealed class WindowsCustomPluginInstallerTests : IDisposable
{
    private readonly string _root = Path.Combine(
        Path.GetTempPath(), "chatos-custom-plugin-tests", Guid.NewGuid().ToString("N"));

    [Fact]
    public async Task PinnedReaNpmMcpPreflightsReacquiresAndPersistsCanonicalComponent()
    {
        var store = new MemoryInstalledPluginStore();
        var clients = new RecordingClientFactory(Tool("binary_session"), Tool("inspect_binary"));
        var acquirer = new FixtureNpmAcquirer();
        var installer = Installer(store, clients, acquirer);
        var draft = new NpmMcpDraft(
            "Reverse Engineer Anything",
            "Reverse engineering tools exposed through REA MCP.",
            "reverse-engineer-anything",
            "rea-agents",
            "6.5.0",
            "rea",
            ["mcp"]);

        var inspection = await installer.InspectNpmMcpAsync(draft);

        Assert.Equal("6.5.0", inspection.Version);
        Assert.Equal("reverse-engineer-anything", Assert.Single(inspection.Components).ComponentKey);
        Assert.Contains("binary_session", inspection.Components[0].ToolNames);
        Assert.Equal(1, acquirer.Calls);
        Assert.Empty(await store.ListAsync());

        var installed = await installer.InstallNpmMcpAsync(draft, inspection.Fingerprint);

        Assert.Equal(2, acquirer.Calls);
        Assert.Equal("customMcp", installed.SourceKind);
        Assert.Contains("rea-agents@6.5.0", installed.SourceSummary, StringComparison.Ordinal);
        Assert.True(File.Exists(Path.Combine(installed.InstallationPath, "node_modules",
            "rea-agents", "scripts", "rea.mjs")));
        Assert.Equal(installed, Assert.Single(await store.ListAsync()));
    }

    [Fact]
    public async Task NpmMcpRejectsFloatingVersionBeforeAcquisition()
    {
        var acquirer = new FixtureNpmAcquirer();
        var installer = Installer(new MemoryInstalledPluginStore(),
            new RecordingClientFactory(Tool("unused")), acquirer);

        await Assert.ThrowsAsync<PluginPackageException>(() => installer.InspectNpmMcpAsync(new(
            "REA", "", "reverse-engineer-anything", "rea-agents", "latest", "rea", ["mcp"])));

        Assert.Equal(0, acquirer.Calls);
    }

    [Fact]
    public async Task RealPinnedReaPackageListsCanonicalToolsAndCanBeReopened()
    {
        if (!string.Equals(
                Environment.GetEnvironmentVariable("CHATOS_RUN_REAL_REA"),
                "1",
                StringComparison.Ordinal)) return;
        var store = new MemoryInstalledPluginStore();
        var runtimeRoot = Path.Combine(_root, "rea-runtime");
        var loader = new PluginManifestLoader(runtimeRoot);
        var installer = new WindowsCustomPluginInstaller(
            Path.Combine(_root, "rea-installs"),
            loader,
            new ActualClientFactory(),
            store);
        var draft = new NpmMcpDraft(
            "Reverse Engineer Anything",
            "Reverse engineering tools exposed through REA MCP.",
            "reverse-engineer-anything",
            "rea-agents",
            "6.5.0",
            "rea",
            ["mcp"]);
        using var timeout = new CancellationTokenSource(TimeSpan.FromMinutes(8));

        var inspection = await installer.InspectNpmMcpAsync(draft, timeout.Token);
        var component = Assert.Single(inspection.Components);
        Assert.True(component.ToolNames.Count >= 200, component.ToolNames.Count.ToString());
        Assert.Contains("binary_session", component.ToolNames);

        var installed = await installer.InstallNpmMcpAsync(
            draft, inspection.Fingerprint, timeout.Token);
        var launch = await loader.PrepareAsync(
            installed,
            draft.ComponentKey,
            draft.ComponentKey,
            $"rea-real-{Guid.NewGuid():N}",
            workspaceRoot: null,
            new HashSet<string>(["process.spawn"], StringComparer.Ordinal),
            "local-user",
            "local-device",
            cancellationToken: timeout.Token);
        await using var client = new PluginStdioClient(launch, new JobProcessLauncher());
        await client.StartAsync(timeout.Token);
        var initialized = await client.InitializeAsync(timeout.Token);
        var binarySession = Assert.Single(initialized.Tools, tool =>
            string.Equals(tool.GetProperty("name").GetString(), "binary_session", StringComparison.Ordinal));
        Assert.Equal("object", binarySession.GetProperty("inputSchema").GetProperty("type").GetString());
        var callResult = await client.CallToolAsync(
            "binary_session",
            JsonSerializer.SerializeToElement(new { }),
            TimeSpan.FromSeconds(30),
            timeout.Token);
        Assert.NotEqual(JsonValueKind.Undefined, callResult.ValueKind);
        Assert.NotEqual(JsonValueKind.Null, callResult.ValueKind);
        Console.WriteLine($"REA_REAL_RESULT tools={initialized.Tools.Count} " +
            $"binary_session_kind={callResult.ValueKind}");
        await client.TerminateAsync();
    }

    [Fact]
    public async Task LocalStdioPluginRequiresMatchingPreflightAndPersistsOnlyAfterRealToolList()
    {
        var source = CreateFixture();
        var store = new MemoryInstalledPluginStore();
        var clients = new RecordingClientFactory(Tool("ping"));
        var installer = Installer(store, clients);
        var draft = new LocalPluginSourceDraft(LocalPluginSourceKind.LocalFolder, source);

        var inspection = await installer.InspectAsync(draft);

        Assert.Equal(64, inspection.Fingerprint.Length);
        Assert.Equal("Local Fixture", inspection.DisplayName);
        Assert.Equal("1.0.0", inspection.Version);
        var component = Assert.Single(inspection.Components);
        Assert.Equal("local-fixture", component.ComponentKey);
        Assert.Equal(["ping"], component.ToolNames);
        Assert.Equal(1, clients.Starts);
        Assert.Equal(1, clients.Initializations);
        Assert.Empty(await store.ListAsync());

        await Assert.ThrowsAsync<PluginPackageException>(() => installer.InstallAsync(
            draft, new string('0', 64)));
        Assert.Empty(await store.ListAsync());
        AssertNoCommittedPackages();

        var record = await installer.InstallAsync(draft, inspection.Fingerprint);

        Assert.Equal(3, clients.Initializations);
        Assert.True(Directory.Exists(record.InstallationPath));
        Assert.True(File.Exists(Path.Combine(record.InstallationPath, "chatos.plugin.json")));
        Assert.Equal(record, Assert.Single(await store.ListAsync()));
        Assert.DoesNotContain(Directory.EnumerateDirectories(
            Path.Combine(_root, "installs", ".custom-staging")), _ => true);
    }

    [Fact]
    public async Task SourceDriftAfterPreflightFailsClosedWithoutReplacingInstalledState()
    {
        var source = CreateFixture();
        var store = new MemoryInstalledPluginStore();
        var clients = new RecordingClientFactory(Tool("ping"));
        var installer = Installer(store, clients);
        var draft = new LocalPluginSourceDraft(LocalPluginSourceKind.LocalFolder, source);
        var inspection = await installer.InspectAsync(draft);

        await File.AppendAllTextAsync(Path.Combine(source, "bin", "fixture.js"), "// drift\n");

        var exception = await Assert.ThrowsAsync<PluginPackageException>(() =>
            installer.InstallAsync(draft, inspection.Fingerprint));
        Assert.Contains("changed after preflight", exception.Message, StringComparison.Ordinal);
        Assert.Empty(await store.ListAsync());
        AssertNoCommittedPackages();
    }

    [Fact]
    public async Task GitSourceUsesPinnedReferenceAndSubdirectoryThenReacquiresBeforeCommit()
    {
        var store = new MemoryInstalledPluginStore();
        var clients = new RecordingClientFactory(Tool("ping"));
        var git = new FixtureGitAcquirer("packages/fixture");
        var installer = Installer(store, clients, gitAcquirer: git);
        var draft = new LocalPluginSourceDraft(
            LocalPluginSourceKind.GitRepository,
            "https://example.test/owner/plugins.git",
            GitReference: "refs/tags/v1.2.3",
            Subdirectory: "packages/fixture");

        var inspection = await installer.InspectAsync(draft);

        Assert.Equal(1, git.Calls);
        Assert.All(git.Requests, request =>
        {
            Assert.Equal(draft.Location, request.Url);
            Assert.Equal(draft.GitReference, request.Reference);
        });
        Assert.Contains("Git · https://example.test/owner/plugins.git · refs/tags/v1.2.3",
            inspection.SourceSummary, StringComparison.Ordinal);
        Assert.Contains("packages/fixture", inspection.SourceSummary, StringComparison.Ordinal);
        Assert.Empty(await store.ListAsync());

        var installed = await installer.InstallAsync(draft, inspection.Fingerprint);

        Assert.Equal(2, git.Calls);
        Assert.Equal("gitRepository", installed.SourceKind);
        Assert.False(Directory.Exists(Path.Combine(installed.InstallationPath, ".git")));
        Assert.True(File.Exists(Path.Combine(installed.InstallationPath, "chatos.plugin.json")));
        Assert.Equal(installed, Assert.Single(await store.ListAsync()));
    }

    [Theory]
    [InlineData("http://example.test/plugin.git", "main")]
    [InlineData("https://token@example.test/plugin.git", "main")]
    [InlineData("https://example.test/plugin.git?token=secret", "main")]
    [InlineData("https://example.test/plugin.git#fragment", "main")]
    [InlineData("https://example.test/plugin.git", "--upload-pack=evil")]
    [InlineData("https://example.test/plugin.git", "refs/heads/main..evil")]
    [InlineData("https://example.test/plugin.git", "refs/heads/main@{1}")]
    public async Task UnsafeGitSourcesFailBeforeAcquisition(string url, string reference)
    {
        var git = new FixtureGitAcquirer();
        var installer = Installer(
            new MemoryInstalledPluginStore(),
            new RecordingClientFactory(Tool("ping")),
            gitAcquirer: git);

        await Assert.ThrowsAsync<PluginPackageException>(() => installer.InspectAsync(new(
            LocalPluginSourceKind.GitRepository,
            url,
            GitReference: reference)));

        Assert.Equal(0, git.Calls);
        AssertNoCommittedPackages();
    }

    [Fact]
    public async Task GitSourceDriftAfterPreflightFailsClosed()
    {
        var git = new FixtureGitAcquirer(driftOnSecondAcquisition: true);
        var store = new MemoryInstalledPluginStore();
        var installer = Installer(store, new RecordingClientFactory(Tool("ping")), gitAcquirer: git);
        var draft = new LocalPluginSourceDraft(
            LocalPluginSourceKind.GitRepository,
            "https://example.test/owner/plugin.git",
            GitReference: "main");
        var inspection = await installer.InspectAsync(draft);

        var exception = await Assert.ThrowsAsync<PluginPackageException>(() =>
            installer.InstallAsync(draft, inspection.Fingerprint));

        Assert.Contains("changed after preflight", exception.Message, StringComparison.Ordinal);
        Assert.Equal(2, git.Calls);
        Assert.Empty(await store.ListAsync());
        AssertNoCommittedPackages();
    }

    [Fact]
    public async Task RealHttpsGitAcquirerFetchesExplicitRemoteRefWithoutShellOrPrompts()
    {
        if (!string.Equals(
                Environment.GetEnvironmentVariable("CHATOS_RUN_REAL_GIT_PLUGIN"),
                "1",
                StringComparison.Ordinal)) return;
        var destination = Path.Combine(_root, "real-git-acquisition");
        Directory.CreateDirectory(destination);
        using var timeout = new CancellationTokenSource(TimeSpan.FromMinutes(2));

        await new WindowsGitPluginSourceAcquirer().AcquireAsync(
            "https://github.com/octocat/Hello-World.git",
            "refs/heads/master",
            destination,
            timeout.Token);

        Assert.True(File.Exists(Path.Combine(destination, ".git", "FETCH_HEAD")));
        Assert.True(File.Exists(Path.Combine(destination, "README")));
        Assert.False(Directory.Exists(Path.Combine(destination, ".git", "hooks", "post-checkout")));
    }

    [Fact]
    public async Task UnsupportedSourcesAndPlaintextEnvironmentFailBeforeAnyRuntimeStarts()
    {
        var store = new MemoryInstalledPluginStore();
        var clients = new RecordingClientFactory(Tool("ping"));
        var installer = Installer(store, clients);

        await Assert.ThrowsAsync<PluginPackageException>(() => installer.InspectAsync(
            new LocalPluginSourceDraft(
                LocalPluginSourceKind.NpmPackage,
                "rea-agents",
                ExactVersion: "6.5.0")));

        var source = CreateFixture(environment: "{\"TOKEN\":\"plaintext\"}");
        await Assert.ThrowsAsync<PluginPackageException>(() => installer.InspectAsync(
            new LocalPluginSourceDraft(LocalPluginSourceKind.LocalFolder, source)));
        Assert.Equal(0, clients.Starts);
        Assert.Empty(await store.ListAsync());
    }

    [Fact]
    public async Task PortableSkillOnlyPluginNormalizesAndPreflightsWithoutStartingRuntime()
    {
        var source = Path.Combine(_root, $"portable-{Guid.NewGuid():N}");
        var skill = Path.Combine(source, "skills", "hello");
        Directory.CreateDirectory(skill);
        File.WriteAllText(Path.Combine(source, "plugin.json"), """
        {
          "$schema": "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
          "name": "portable-hello",
          "version": "1.2.3",
          "description": "Portable skill fixture",
          "extensions": { "com.openai": { "interface": { "displayName": "Portable Hello" } } }
        }
        """);
        File.WriteAllText(Path.Combine(skill, "SKILL.md"), """
        ---
        name: hello
        description: Greet the user using the portable plugin workflow.
        ---
        Greet the user warmly.
        """);
        var store = new MemoryInstalledPluginStore();
        var clients = new RecordingClientFactory(Tool("unused"));
        var installer = Installer(store, clients);
        var draft = new LocalPluginSourceDraft(LocalPluginSourceKind.LocalFolder, source);

        var inspection = await installer.InspectAsync(draft);

        Assert.Equal("Portable Hello", inspection.DisplayName);
        Assert.Equal("1.2.3", inspection.Version);
        Assert.Contains("Agent Plugin", inspection.SourceSummary, StringComparison.Ordinal);
        var component = Assert.Single(inspection.Components);
        Assert.Equal("hello", component.ComponentKey);
        Assert.Equal(LocalPluginInspectionComponentKind.Skill, component.Kind);
        Assert.Empty(component.ToolNames);
        Assert.Equal(0, clients.Starts);

        var record = await installer.InstallAsync(draft, inspection.Fingerprint);

        Assert.Equal(0, clients.Starts);
        Assert.True(File.Exists(Path.Combine(record.InstallationPath, "chatos.plugin.json")));
        Assert.True(File.Exists(Path.Combine(record.InstallationPath, "skills", "hello", "SKILL.md")));
        Assert.Contains("chatos.plugin.json", record.PackageFileSha256!.Keys);
        Assert.Contains("skills/hello/SKILL.md", record.PackageFileSha256.Keys);
    }

    [Theory]
    [InlineData(".cursor/mcp.json", "mcpServers")]
    [InlineData(".vscode/mcp.json", "servers")]
    [InlineData(".gemini/settings.json", "mcpServers")]
    [InlineData("claude_desktop_config.json", "mcpServers")]
    [InlineData("cline_mcp_settings.json", "mcpServers")]
    [InlineData("opencode.json", "mcp")]
    public async Task MainstreamMcpConfigurationLayoutsNormalizeToOneSafeStdioContract(
        string relativeConfigPath,
        string serversProperty)
    {
        var source = Path.Combine(_root, $"mcp-config-{Guid.NewGuid():N}");
        Directory.CreateDirectory(Path.GetDirectoryName(Path.Combine(source, relativeConfigPath))!);
        File.WriteAllText(Path.Combine(source, relativeConfigPath), $$"""
        {
          "{{serversProperty}}": {
            "fixture": {
              "type": "{{(serversProperty == "mcp" ? "local" : "stdio")}}",
              "command": ["node", "${PLUGIN_ROOT}/server.js"],
              "args": ["--fixture"]
            }
          }
        }
        """);
        File.WriteAllText(Path.Combine(source, "server.js"), "console.log('fixture');\n");
        var store = new MemoryInstalledPluginStore();
        var clients = new RecordingClientFactory(Tool("ping"));
        var installer = Installer(store, clients);
        var draft = new LocalPluginSourceDraft(LocalPluginSourceKind.LocalFolder, source);

        var inspection = await installer.InspectAsync(draft);

        Assert.Contains("Standalone MCP", inspection.SourceSummary, StringComparison.Ordinal);
        var component = Assert.Single(inspection.Components);
        Assert.Equal("fixture", component.ComponentKey);
        Assert.Equal(LocalPluginInspectionComponentKind.Mcp, component.Kind);
        Assert.Equal(["ping"], component.ToolNames);
        Assert.Equal(1, clients.Starts);

        var record = await installer.InstallAsync(draft, inspection.Fingerprint);
        using var manifest = JsonDocument.Parse(
            await File.ReadAllBytesAsync(Path.Combine(record.InstallationPath, "chatos.plugin.json")));
        var normalized = manifest.RootElement.GetProperty("mcpServers").GetProperty("fixture");
        Assert.Equal("compat-fixture", normalized.GetProperty("bin").GetString());
        Assert.Equal<string>(
            ["${PLUGIN_ROOT}/server.js", "--fixture"],
            normalized.GetProperty("args").EnumerateArray().Select(value => value.GetString()!).ToArray());
        Assert.True(File.Exists(Path.Combine(record.InstallationPath, "bin", "compat-fixture.mjs")));
        Assert.Contains("bin/compat-fixture.mjs", record.PackageFileSha256!.Keys);
    }

    [Fact]
    public async Task CodexPluginIdentityImportsMcpAndRemoteTransportFailsClosed()
    {
        var source = Path.Combine(_root, $"codex-{Guid.NewGuid():N}");
        Directory.CreateDirectory(Path.Combine(source, ".codex-plugin"));
        File.WriteAllText(Path.Combine(source, ".codex-plugin", "plugin.json"), """
        { "name": "codex-fixture", "version": "2.0.0", "displayName": "Codex Fixture" }
        """);
        File.WriteAllText(Path.Combine(source, ".mcp.json"), """
        { "mcpServers": { "fixture": { "command": "node", "args": ["${PLUGIN_ROOT}/server.js"] } } }
        """);
        File.WriteAllText(Path.Combine(source, "server.js"), "console.log('fixture');\n");
        var clients = new RecordingClientFactory(Tool("ping"));
        var installer = Installer(new MemoryInstalledPluginStore(), clients);

        var inspection = await installer.InspectAsync(
            new LocalPluginSourceDraft(LocalPluginSourceKind.LocalFolder, source));

        Assert.Equal("Codex Fixture", inspection.DisplayName);
        Assert.Contains("Codex Plugin", inspection.SourceSummary, StringComparison.Ordinal);

        File.WriteAllText(Path.Combine(source, ".mcp.json"), """
        { "mcpServers": { "fixture": { "type": "sse", "url": "https://example.test/mcp" } } }
        """);
        var exception = await Assert.ThrowsAsync<PluginPackageException>(() => installer.InspectAsync(
            new LocalPluginSourceDraft(LocalPluginSourceKind.LocalFolder, source)));
        Assert.Contains("only local stdio", exception.Message, StringComparison.Ordinal);
        Assert.Equal(1, clients.Starts);
    }

    [Fact]
    public async Task WindowsRuntimePerformsActualInitializeAndToolsListInJobControlledProcess()
    {
        if (!OperatingSystem.IsWindows()) return;
        var source = CreateFixture(actualMcp: true);
        var store = new MemoryInstalledPluginStore();
        var installer = Installer(store, new ActualClientFactory());

        var inspection = await installer.InspectAsync(
            new LocalPluginSourceDraft(LocalPluginSourceKind.LocalFolder, source));

        var component = Assert.Single(inspection.Components);
        Assert.Equal(["ping"], component.ToolNames);
        Assert.Empty(await store.ListAsync());
        var runtimeRoot = Path.Combine(_root, "runtime");
        Assert.True(Directory.Exists(runtimeRoot));
    }

    [Fact]
    public async Task ImportedVsCodeMcpPerformsActualInitializeAndToolsListInJobControlledProcess()
    {
        if (!OperatingSystem.IsWindows()) return;
        var source = Path.Combine(_root, $"actual-import-{Guid.NewGuid():N}");
        Directory.CreateDirectory(Path.Combine(source, ".vscode"));
        File.WriteAllText(Path.Combine(source, ".vscode", "mcp.json"), """
        { "servers": { "fixture": { "type": "stdio", "command": ["node", "${PLUGIN_ROOT}/server.js"] } } }
        """);
        File.WriteAllText(Path.Combine(source, "server.js"), """
        const readline = require("node:readline");
        const lines = readline.createInterface({ input: process.stdin });
        lines.on("line", line => {
          const request = JSON.parse(line);
          if (request.id === undefined) return;
          let result = {};
          if (request.method === "initialize") {
            result = { protocolVersion: "2024-11-05", capabilities: {}, serverInfo: { name: "fixture", version: "1" } };
          } else if (request.method === "tools/list") {
            result = { tools: [{ name: "ping", inputSchema: { type: "object" } }] };
          }
          process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: request.id, result }) + "\n");
        });
        """);
        var installer = Installer(new MemoryInstalledPluginStore(), new ActualClientFactory());

        var inspection = await installer.InspectAsync(
            new LocalPluginSourceDraft(LocalPluginSourceKind.LocalFolder, source));

        Assert.Equal(["ping"], Assert.Single(inspection.Components).ToolNames);
    }

    [Fact]
    public async Task WindowsProductionAppContainerPreflightIsBoundedAndLeavesNoResidue()
    {
        if (!OperatingSystem.IsWindows()) return;
        var source = CreateFixture(actualMcp: true);
        var store = new MemoryInstalledPluginStore();
        var clients = new ProductionClientFactory();
        var installer = Installer(store, clients);
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(45));

        LocalPluginInspection? inspection = null;
        PluginRuntimeException? missingVolumeTraverse = null;
        var stopwatch = Stopwatch.StartNew();
        try
        {
            inspection = await installer.InspectAsync(
                new LocalPluginSourceDraft(LocalPluginSourceKind.LocalFolder, source),
                timeout.Token);
        }
        catch (PluginRuntimeException exception) when (
            exception.Message.Contains("EPERM", StringComparison.Ordinal) &&
            exception.Message.Contains("lstat 'C:\\'", StringComparison.Ordinal))
        {
            // This host has no administrator-preconfigured package traverse ACE on the
            // workspace volume. Keep the machine prerequisite explicit while verifying that
            // a failed real production launch is bounded and fully compensated.
            missingVolumeTraverse = exception;
        }
        stopwatch.Stop();

        Assert.True(stopwatch.Elapsed < TimeSpan.FromSeconds(15), stopwatch.Elapsed.ToString());
        Assert.True(inspection is not null || missingVolumeTraverse is not null);
        if (inspection is not null)
        {
            Assert.Equal(["ping"], Assert.Single(inspection.Components).ToolNames);
        }
        Assert.NotEmpty(clients.ProfileNames);
        Assert.All(
            clients.ProfileNames,
            profileName => Assert.False(
                WindowsAppContainerSandbox.HasPendingProfileCleanup(profileName),
                profileName));
        Assert.DoesNotContain(
            clients.ProfileNames,
            profileName => Directory.Exists(Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "Packages",
                profileName.ToLowerInvariant())));
    }

    public void Dispose()
    {
        if (Directory.Exists(_root)) DeleteTestDirectory(_root);
    }

    private static void DeleteTestDirectory(string path)
    {
        foreach (var entry in Directory.EnumerateFileSystemEntries(path))
        {
            var attributes = File.GetAttributes(entry);
            if ((attributes & FileAttributes.Directory) != 0 &&
                (attributes & FileAttributes.ReparsePoint) == 0)
            {
                DeleteTestDirectory(entry);
                continue;
            }
            File.SetAttributes(entry, attributes & ~FileAttributes.ReadOnly);
            if ((attributes & FileAttributes.Directory) != 0)
            {
                Directory.Delete(entry, recursive: false);
            }
            else
            {
                File.Delete(entry);
            }
        }
        File.SetAttributes(path, File.GetAttributes(path) & ~FileAttributes.ReadOnly);
        Directory.Delete(path, recursive: false);
    }

    private WindowsCustomPluginInstaller Installer(
        IInstalledPluginStore store,
        IPluginMcpClientFactory clients,
        INpmMcpPackageAcquirer? npmAcquirer = null,
        IGitPluginSourceAcquirer? gitAcquirer = null) => new(
            Path.Combine(_root, "installs"),
            new PluginManifestLoader(Path.Combine(_root, "runtime")),
            clients,
            store,
            npmAcquirer,
            gitAcquirer);

    private sealed class FixtureGitAcquirer(
        string? subdirectory = null,
        bool driftOnSecondAcquisition = false) : IGitPluginSourceAcquirer
    {
        public int Calls { get; private set; }
        public List<(string Url, string Reference)> Requests { get; } = [];

        public async Task AcquireAsync(
            string repositoryUrl,
            string reference,
            string destinationRoot,
            CancellationToken cancellationToken)
        {
            Calls++;
            Requests.Add((repositoryUrl, reference));
            var root = destinationRoot;
            if (subdirectory is not null)
            {
                root = Path.Combine(destinationRoot,
                    subdirectory.Replace('/', Path.DirectorySeparatorChar));
            }
            Directory.CreateDirectory(Path.Combine(destinationRoot, ".git", "hooks"));
            var gitConfig = Path.Combine(destinationRoot, ".git", "config");
            await File.WriteAllTextAsync(gitConfig, "[core]\nrepositoryformatversion = 0\n", cancellationToken);
            File.SetAttributes(gitConfig, File.GetAttributes(gitConfig) | FileAttributes.ReadOnly);
            Directory.CreateDirectory(Path.Combine(root, "bin"));
            await File.WriteAllTextAsync(Path.Combine(root, "package.json"), """
            { "name": "git-fixture", "version": "1.2.3", "bin": { "fixture": "bin/fixture.js" } }
            """, cancellationToken);
            await File.WriteAllTextAsync(Path.Combine(root, "bin", "fixture.js"),
                driftOnSecondAcquisition && Calls > 1
                    ? "console.log('drift');\n"
                    : "console.log('fixture');\n",
                cancellationToken);
            await File.WriteAllTextAsync(Path.Combine(root, "chatos.plugin.json"), """
            {
              "schemaVersion": 3,
              "name": "git-fixture",
              "version": "1.2.3",
              "description": "Git fixture",
              "interface": { "displayName": "Git Fixture" },
              "mcpServers": {
                "git-fixture": {
                  "type": "stdio",
                  "bin": "fixture",
                  "args": [],
                  "env": {},
                  "agent": { "displayName": "Git Fixture" }
                }
              },
              "permissions": [
                { "permission": "process.spawn", "required": true, "components": ["git-fixture"] }
              ]
            }
            """, cancellationToken);
        }
    }

    private sealed class FixtureNpmAcquirer : INpmMcpPackageAcquirer
    {
        public int Calls { get; private set; }

        public async Task AcquireAsync(
            NpmMcpDraft draft,
            string packageRoot,
            CancellationToken cancellationToken)
        {
            Calls++;
            using (var wrapper = JsonDocument.Parse(await File.ReadAllBytesAsync(
                       Path.Combine(packageRoot, "package.json"), cancellationToken)))
            {
                Assert.Equal(draft.PackageVersion,
                    wrapper.RootElement.GetProperty("dependencies")
                        .GetProperty(draft.PackageName).GetString());
            }
            var root = Path.Combine(packageRoot, "node_modules", "rea-agents");
            Directory.CreateDirectory(Path.Combine(root, "scripts"));
            await File.WriteAllTextAsync(Path.Combine(root, "package.json"), """
            {
              "name": "rea-agents",
              "version": "6.5.0",
              "bin": { "rea": "scripts/rea.mjs" }
            }
            """, cancellationToken);
            await File.WriteAllTextAsync(
                Path.Combine(root, "scripts", "rea.mjs"),
                "console.log('fixture');\n",
                cancellationToken);
        }
    }

    private string CreateFixture(string environment = "{}", bool actualMcp = false)
    {
        var source = Path.Combine(_root, $"source-{Guid.NewGuid():N}");
        Directory.CreateDirectory(Path.Combine(source, "bin"));
        File.WriteAllText(Path.Combine(source, "package.json"), """
        {
          "name": "local-fixture",
          "version": "1.0.0",
          "bin": { "fixture": "bin/fixture.js" }
        }
        """);
        File.WriteAllText(Path.Combine(source, "bin", "fixture.js"), actualMcp ? """
        const readline = require("node:readline");
        const lines = readline.createInterface({ input: process.stdin });
        lines.on("line", line => {
          const request = JSON.parse(line);
          if (request.id === undefined) return;
          let result = {};
          if (request.method === "initialize") {
            result = { protocolVersion: "2024-11-05", capabilities: {}, serverInfo: { name: "fixture", version: "1" } };
          } else if (request.method === "tools/list") {
            result = { tools: [{ name: "ping", description: "Ping", inputSchema: { type: "object", properties: {} } }] };
          }
          process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: request.id, result }) + "\n");
        });
        """ : "console.log('fixture');\n");
        File.WriteAllText(Path.Combine(source, "chatos.plugin.json"), $$"""
        {
          "schemaVersion": 3,
          "name": "local-fixture",
          "version": "1.0.0",
          "description": "Local fixture",
          "interface": { "displayName": "Local Fixture" },
          "mcpServers": {
            "local-fixture": {
              "type": "stdio",
              "bin": "fixture",
              "args": [],
              "env": {{environment}},
              "agent": { "displayName": "Local Fixture" }
            }
          },
          "permissions": [
            { "permission": "process.spawn", "required": true, "components": ["local-fixture"] }
          ]
        }
        """);
        return source;
    }

    private void AssertNoCommittedPackages()
    {
        var root = Path.Combine(_root, "installs");
        if (!Directory.Exists(root)) return;
        Assert.DoesNotContain(
            Directory.EnumerateDirectories(root),
            path => !Path.GetFileName(path).StartsWith(".", StringComparison.Ordinal));
    }

    private static JsonElement Tool(string name) => JsonSerializer.SerializeToElement(new
    {
        name,
        description = "Ping",
        inputSchema = new { type = "object", properties = new { } },
    });

    private sealed class RecordingClientFactory(params JsonElement[] tools) : IPluginMcpClientFactory
    {
        public int Starts { get; private set; }
        public int Initializations { get; private set; }

        public IPluginMcpClient Create(PreparedPluginLaunch launch) => new Client(this, tools);

        private sealed class Client(RecordingClientFactory owner, IReadOnlyList<JsonElement> tools)
            : IPluginMcpClient
        {
            public Task StartAsync(CancellationToken cancellationToken = default)
            {
                owner.Starts++;
                return Task.CompletedTask;
            }

            public Task<PluginMcpInitialization> InitializeAsync(
                CancellationToken cancellationToken = default)
            {
                owner.Initializations++;
                return Task.FromResult(new PluginMcpInitialization(null, tools));
            }

            public Task<JsonElement> CallToolAsync(
                string name,
                JsonElement arguments,
                TimeSpan timeout,
                CancellationToken cancellationToken = default) => throw new NotSupportedException();

            public Task TerminateAsync() => Task.CompletedTask;

            public ValueTask DisposeAsync() => ValueTask.CompletedTask;
        }
    }

    private sealed class ActualClientFactory : IPluginMcpClientFactory
    {
        public IPluginMcpClient Create(PreparedPluginLaunch launch) =>
            new PluginStdioClient(launch, new JobProcessLauncher());
    }

    private sealed class ProductionClientFactory : IPluginMcpClientFactory
    {
        public List<string> ProfileNames { get; } = [];

        public IPluginMcpClient Create(PreparedPluginLaunch launch)
        {
            var processLaunch = PluginProcessLaunchRequest.From(launch);
            var policy = WindowsPluginProcessLauncher.InstallationPolicy(processLaunch);
            ProfileNames.Add(WindowsAppContainerSandbox.ProfileName(
                processLaunch.InstallationPath,
                policy.PermissionProfile,
                $"plugin:{processLaunch.Record.PluginId}:" +
                $"{processLaunch.Record.ReleaseId}:{processLaunch.ComponentKey}"));
            return new PluginStdioClient(launch);
        }
    }

    private sealed class JobProcessLauncher : IPluginProcessLauncher
    {
        public Task<IPluginProcess> LaunchAsync(
            PluginProcessLaunchRequest launch,
            CancellationToken cancellationToken = default)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var start = new ProcessStartInfo
            {
                FileName = launch.ExecutablePath,
                WorkingDirectory = launch.InstallationPath,
                UseShellExecute = false,
                CreateNoWindow = true,
                RedirectStandardInput = true,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
            };
            foreach (var argument in launch.Arguments) start.ArgumentList.Add(argument);
            foreach (var pair in launch.Environment) start.Environment[pair.Key] = pair.Value;
            var process = Process.Start(start)
                ?? throw new InvalidOperationException("Unable to start the MCP fixture.");
            SafeKernelObjectHandle? job = null;
            try
            {
                job = NativeConPty.CreateKillOnCloseJob();
                NativeConPty.ThrowIfFalse(NativeConPty.AssignProcessToJobObject(job, process.SafeHandle));
                var result = new JobProcess(process, job);
                job = null;
                return Task.FromResult<IPluginProcess>(result);
            }
            finally
            {
                job?.Dispose();
            }
        }

        private sealed class JobProcess(Process process, SafeKernelObjectHandle job) : IPluginProcess
        {
            private int _terminated;
            public Stream StandardInput => process.StandardInput.BaseStream;
            public Stream StandardOutput => process.StandardOutput.BaseStream;
            public Stream StandardError => process.StandardError.BaseStream;
            public bool HasExited => process.HasExited;

            public async Task<int> WaitForExitAsync(CancellationToken cancellationToken = default)
            {
                await process.WaitForExitAsync(cancellationToken);
                return process.ExitCode;
            }

            public Task TerminateAsync()
            {
                if (Interlocked.Exchange(ref _terminated, 1) == 0)
                {
                    NativeConPty.TerminateJob(job, 1);
                }
                return Task.CompletedTask;
            }

            public async ValueTask DisposeAsync()
            {
                await TerminateAsync();
                process.Dispose();
                job.Dispose();
            }
        }
    }

    private sealed class MemoryInstalledPluginStore : IInstalledPluginStore
    {
        private readonly Dictionary<string, InstalledPluginRecord> _records = new(StringComparer.Ordinal);

        public Task<IReadOnlyList<InstalledPluginRecord>> ListAsync(
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<InstalledPluginRecord>>(_records.Values.ToArray());

        public Task<InstalledPluginRecord?> GetAsync(
            string pluginId,
            CancellationToken cancellationToken = default) =>
            Task.FromResult(_records.GetValueOrDefault(pluginId));

        public Task SaveAsync(
            InstalledPluginRecord record,
            CancellationToken cancellationToken = default)
        {
            _records[record.PluginId] = record;
            return Task.CompletedTask;
        }

        public Task DeleteAsync(string pluginId, CancellationToken cancellationToken = default)
        {
            _records.Remove(pluginId);
            return Task.CompletedTask;
        }
    }
}
