using System.Text.Json;
using ChatOS.Connector.LocalAgent;
using ChatOS.Connector.Plugins;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentPluginInstallerToolExecutorTests
{
    [Fact]
    public void PlatformWorkerSurfacesOnlyTrustedInstallerValidationErrors()
    {
        Assert.Equal(
            "The local Plugin source directory does not exist.",
            WindowsLocalAgentPlatformToolWorker.SafeErrorSummary(
                new PluginPackageException(
                    "The local Plugin source directory does not exist.")));
        Assert.Equal(
            "Local platform tool failed.",
            WindowsLocalAgentPlatformToolWorker.SafeErrorSummary(
                new IOException(@"Access failed for C:\private\source.")));
        Assert.Equal(
            "Local platform tool failed; sensitive details were hidden.",
            WindowsLocalAgentPlatformToolWorker.SafeErrorSummary(
                new PluginPackageException("The API_KEY field is invalid.")));
    }

    [Fact]
    public async Task DedicatedInstallerConversationCanInspectLocalSource()
    {
        var host = new InstallerHost(WindowsLocalAgentWorkspaceService.PluginInstallerContactId);
        var installer = new Installer();
        var executor = new WindowsLocalAgentPluginInstallerToolExecutor(
            host,
            new WindowsLocalAgentConversationClient(host),
            installer);

        var output = await executor.ExecuteAsync(
            "user-1",
            Invocation(
                WindowsLocalAgentCapabilityCatalog.PluginInstallerInspectSourceToolName,
                sideEffecting: false,
                """{"source_kind":"local_folder","location":"C:\\fixture\\plugin"}"""),
            CancellationToken.None);

        Assert.Equal("inspection_passed", output.GetProperty("status").GetString());
        Assert.Equal("C:\\fixture\\plugin", installer.LastInspected?.Location);
        Assert.Equal("tool-one", output.GetProperty("components")[0]
            .GetProperty("tool_names")[0].GetString());
    }

    [Fact]
    public async Task DedicatedInstallerConversationPreservesHttpsGitIdentityAcrossInspectAndInstall()
    {
        var host = new InstallerHost(WindowsLocalAgentWorkspaceService.PluginInstallerContactId);
        var installer = new Installer();
        var executor = new WindowsLocalAgentPluginInstallerToolExecutor(
            host,
            new WindowsLocalAgentConversationClient(host),
            installer);
        const string source = """
        {
          "source_kind":"git_repository",
          "location":"https://example.test/owner/plugins.git",
          "git_reference":"refs/tags/v1.2.3",
          "subdirectory":"plugins/fixture"
        }
        """;

        var inspected = await executor.ExecuteAsync(
            "user-1",
            Invocation(
                WindowsLocalAgentCapabilityCatalog.PluginInstallerInspectSourceToolName,
                sideEffecting: false,
                source),
            CancellationToken.None);

        Assert.Equal(LocalPluginSourceKind.GitRepository, installer.LastInspected?.Kind);
        Assert.Equal("refs/tags/v1.2.3", installer.LastInspected?.GitReference);
        Assert.Equal("plugins/fixture", installer.LastInspected?.Subdirectory);
        Assert.Equal("git_repository", inspected.GetProperty("resolved_source")
            .GetProperty("source_kind").GetString());

        using var sourceDocument = JsonDocument.Parse(source);
        var install = sourceDocument.RootElement.EnumerateObject()
            .ToDictionary(property => property.Name, property => property.Value.Clone());
        install["expected_fingerprint"] = JsonSerializer.SerializeToElement(new string('a', 64));
        var installed = await executor.ExecuteAsync(
            "user-1",
            Invocation(
                WindowsLocalAgentCapabilityCatalog.PluginInstallerInstallSourceToolName,
                sideEffecting: true,
                JsonSerializer.Serialize(install)),
            CancellationToken.None);

        Assert.Equal("installed", installed.GetProperty("status").GetString());
        Assert.Equal(installer.LastInspected, installer.LastInstalledSource);
        Assert.Equal(new string('a', 64), installer.LastFingerprint);
    }

    [Fact]
    public async Task DedicatedInstallerConversationCarriesCanonicalReaRequestAndFingerprint()
    {
        var host = new InstallerHost(WindowsLocalAgentWorkspaceService.PluginInstallerContactId);
        var installer = new Installer();
        var executor = new WindowsLocalAgentPluginInstallerToolExecutor(
            host,
            new WindowsLocalAgentConversationClient(host),
            installer);
        var request = """
        {
          "display_name":"Reverse Engineer Anything",
          "description":"REA MCP",
          "component_key":"reverse-engineer-anything",
          "package_name":"rea-agents",
          "package_version":"6.5.0",
          "binary_name":"rea",
          "arguments":["mcp"]
        }
        """;

        var inspection = await executor.ExecuteAsync(
            "user-1",
            Invocation(
                WindowsLocalAgentCapabilityCatalog.PluginInstallerInspectNpmToolName,
                sideEffecting: false,
                request),
            CancellationToken.None);

        Assert.Equal("binary_session", inspection.GetProperty("components")[0]
            .GetProperty("tool_names")[0].GetString());
        Assert.Equal("rea-agents", installer.LastNpmInspected?.PackageName);
        Assert.Equal(["mcp"], installer.LastNpmInspected?.Arguments);

        using var installArguments = JsonDocument.Parse(request);
        var withFingerprint = JsonSerializer.SerializeToElement(
            installArguments.RootElement.EnumerateObject()
                .ToDictionary(property => property.Name, property => property.Value.Clone()))
            .Deserialize<Dictionary<string, JsonElement>>()!;
        withFingerprint["expected_fingerprint"] = JsonSerializer.SerializeToElement(new string('c', 64));
        var installed = await executor.ExecuteAsync(
            "user-1",
            Invocation(
                WindowsLocalAgentCapabilityCatalog.PluginInstallerInstallNpmToolName,
                sideEffecting: true,
                JsonSerializer.Serialize(withFingerprint)),
            CancellationToken.None);

        Assert.Equal("installed", installed.GetProperty("status").GetString());
        Assert.Equal(new string('c', 64), installer.LastFingerprint);
    }

    [Fact]
    public async Task OrdinaryConversationCannotUseInstallerTools()
    {
        var host = new InstallerHost(WindowsLocalAgentWorkspaceService.MainContactId);
        var installer = new Installer();
        var executor = new WindowsLocalAgentPluginInstallerToolExecutor(
            host,
            new WindowsLocalAgentConversationClient(host),
            installer);

        await Assert.ThrowsAsync<InvalidOperationException>(() => executor.ExecuteAsync(
            "user-1",
            Invocation(
                WindowsLocalAgentCapabilityCatalog.PluginInstallerInspectSourceToolName,
                sideEffecting: false,
                """{"source_kind":"local_folder","location":"C:\\fixture\\plugin"}"""),
            CancellationToken.None));
        Assert.Null(installer.LastInspected);
    }

    [Fact]
    public async Task InstallRequiresSideEffectingApprovalExemptClassificationAndExactFingerprint()
    {
        var host = new InstallerHost(WindowsLocalAgentWorkspaceService.PluginInstallerContactId);
        var installer = new Installer();
        var executor = new WindowsLocalAgentPluginInstallerToolExecutor(
            host,
            new WindowsLocalAgentConversationClient(host),
            installer);
        var arguments = $$"""
            {"source_kind":"local_folder","location":"C:\\fixture\\plugin","expected_fingerprint":"{{new string('a', 64)}}"}
            """;

        await Assert.ThrowsAsync<InvalidOperationException>(() => executor.ExecuteAsync(
            "user-1",
            Invocation(
                WindowsLocalAgentCapabilityCatalog.PluginInstallerInstallSourceToolName,
                sideEffecting: false,
                arguments),
            CancellationToken.None));

        var output = await executor.ExecuteAsync(
            "user-1",
            Invocation(
                WindowsLocalAgentCapabilityCatalog.PluginInstallerInstallSourceToolName,
                sideEffecting: true,
                arguments),
            CancellationToken.None);

        Assert.Equal("installed", output.GetProperty("status").GetString());
        Assert.Equal(new string('a', 64), installer.LastFingerprint);
    }

    private static WindowsLocalToolInvocation Invocation(
        string toolName,
        bool sideEffecting,
        string arguments) => new(
        "invocation-1",
        "run-1",
        "batch-1",
        "call-1",
        toolName,
        JsonDocument.Parse(arguments).RootElement.Clone(),
        sideEffecting,
        false,
        "not_required",
        null,
        null,
        null,
        "claimed",
        null,
        null,
        1,
        "claim-1",
        null,
        1_000,
        1_000);

    private sealed class InstallerHost(string contactId) : ILocalAgentHostClient
    {
        public string? ActiveOwnerUserId => "user-1";

        public Task StartForOwnerAsync(
            string ownerUserId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task RestartForOwnerAsync(
            string ownerUserId,
            IReadOnlyDictionary<string, string> credentialEnvironment,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task StopAsync(CancellationToken cancellationToken = default) =>
            Task.CompletedTask;

        public Task<TResponse> SendAsync<TCommand, TResponse>(
            TCommand command,
            CancellationToken cancellationToken = default)
            where TCommand : notnull
        {
            object response = command switch
            {
                GetLocalRunCommand => new GetLocalRunResult(
                    "run",
                    new WindowsLocalAgentRun(
                        "run-1",
                        "user-1",
                        "conversation",
                        "conversation-1",
                        "main_chat",
                        JsonSerializer.SerializeToElement(new
                        {
                            source_conversation_id = "conversation-1",
                        }),
                        "running",
                        1,
                        null,
                        1_000,
                        1_000)),
                GetLocalConversationCommand => new LocalConversationResult(
                    "conversation",
                    new WindowsLocalConversationDetail(
                        new WindowsLocalConversationRecord(
                            "conversation-1",
                            "user-1",
                            "Installer",
                            1,
                            1_000,
                            1_000,
                            new WindowsLocalConversationResourceBinding("contact", contactId)),
                        [],
                        [],
                        [])),
                _ => throw new InvalidOperationException(
                    $"Unexpected command: {typeof(TCommand).Name}"),
            };
            return Task.FromResult((TResponse)response);
        }
    }

    private sealed class Installer : ILocalPluginSourceInstaller
    {
        public LocalPluginSourceDraft? LastInspected { get; private set; }
        public LocalPluginSourceDraft? LastInstalledSource { get; private set; }
        public NpmMcpDraft? LastNpmInspected { get; private set; }
        public string? LastFingerprint { get; private set; }

        public Task<LocalPluginInspection> InspectAsync(
            LocalPluginSourceDraft source,
            CancellationToken cancellationToken = default)
        {
            LastInspected = source;
            return Task.FromResult(new LocalPluginInspection(
                new string('a', 64),
                "Fixture",
                "1.0.0",
                "Local folder",
                [new LocalPluginInspectionComponent(
                    "fixture-mcp",
                    "Fixture MCP",
                    LocalPluginInspectionComponentKind.Mcp,
                    ["tool-one"])]));
        }

        public Task<InstalledPluginRecord> InstallAsync(
            LocalPluginSourceDraft source,
            string expectedFingerprint,
            CancellationToken cancellationToken = default)
        {
            LastInstalledSource = source;
            LastFingerprint = expectedFingerprint;
            return Task.FromResult(new InstalledPluginRecord(
                "plugin-1",
                "release-1",
                "1.0.0",
                new string('b', 64),
                "C:\\installed\\plugin-1",
                DateTimeOffset.UnixEpoch,
                [],
                PluginKey: "fixture@local"));
        }

        public Task<LocalPluginInspection> InspectNpmMcpAsync(
            NpmMcpDraft draft,
            CancellationToken cancellationToken = default)
        {
            LastNpmInspected = draft;
            return Task.FromResult(new LocalPluginInspection(
                new string('c', 64),
                draft.DisplayName,
                draft.PackageVersion,
                $"npm {draft.PackageName}@{draft.PackageVersion}",
                [new LocalPluginInspectionComponent(
                    draft.ComponentKey,
                    draft.DisplayName,
                    LocalPluginInspectionComponentKind.Mcp,
                    ["binary_session"])]));
        }

        public Task<InstalledPluginRecord> InstallNpmMcpAsync(
            NpmMcpDraft draft,
            string expectedFingerprint,
            CancellationToken cancellationToken = default)
        {
            LastNpmInspected = draft;
            LastFingerprint = expectedFingerprint;
            return Task.FromResult(new InstalledPluginRecord(
                "plugin-rea",
                "release-rea",
                draft.PackageVersion,
                new string('d', 64),
                "C:\\installed\\rea",
                DateTimeOffset.UnixEpoch,
                [],
                PluginKey: "rea-agents@local"));
        }
    }
}
