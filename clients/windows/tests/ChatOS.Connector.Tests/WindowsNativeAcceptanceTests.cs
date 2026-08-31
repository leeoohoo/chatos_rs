using System.Runtime.InteropServices;
using System.Diagnostics;
using ChatOS.Connector.NetworkGuard;
using ChatOS.Connector.Persistence;
using ChatOS.Connector.Sandbox;
using ChatOS.Connector.Terminal;
using ChatOS.NetworkGuard.Contracts;

namespace ChatOS.Connector.Tests;

[Trait("Category", "WindowsNative")]
public sealed class WindowsNativeAcceptanceTests
{
    [Fact]
    public async Task NativeCommandExecutorCapturesOutputOnWindows()
    {
        if (!OperatingSystem.IsWindows()) return;

        using var workspace = TemporaryDirectory.Create();
        var executor = new WindowsTerminalCommandExecutor();
        var result = await executor.ExecuteAsync(new TerminalCommandRequest(
            CommandInterpreter(),
            ["/d", "/s", "/c", "echo CHATOS_NATIVE_EXEC_OK"],
            workspace.Path,
            workspace.Path,
            "native-exec",
            10_000));

        Assert.True(result.Success, result.Error ?? result.StandardError);
        Assert.Equal(0, result.ExitCode);
        Assert.Contains("CHATOS_NATIVE_EXEC_OK", result.StandardOutput, StringComparison.Ordinal);
        Assert.Equal(ConnectorSandboxPermissionProfile.FullAccess.ToString(), result.SandboxProfile);
        Assert.Equal(ConnectorSandboxNetworkAccess.Host.ToString(), result.SandboxNetwork);
    }

    [Fact]
    public async Task NativeCommandTimeoutReclaimsChildProcessTreeOnWindows()
    {
        if (!OperatingSystem.IsWindows()) return;

        using var workspace = TemporaryDirectory.Create();
        var marker = System.IO.Path.Combine(workspace.Path, "escaped-child.txt");
        var script = System.IO.Path.Combine(workspace.Path, "spawn-child.cmd");
        await File.WriteAllTextAsync(script, $$"""
            @echo off
            start "" /b cmd.exe /d /s /c "ping 127.0.0.1 -n 6 > nul & echo escaped > escaped-child.txt"
            ping 127.0.0.1 -n 30 > nul
            """);

        var executor = new WindowsTerminalCommandExecutor();
        var result = await executor.ExecuteAsync(new TerminalCommandRequest(
            script,
            [],
            workspace.Path,
            workspace.Path,
            "native-job",
            1_000));

        Assert.True(result.TimedOut, result.Error ?? result.StandardError);
        await Task.Delay(TimeSpan.FromSeconds(7));
        Assert.False(File.Exists(marker), "A child process escaped the kill-on-close Job Object.");
    }

    [Fact]
    public async Task AppContainerEnforcesWorkspaceAclAndNetworkCapabilitiesOnWindows()
    {
        if (!OperatingSystem.IsWindows()) return;

        using var workspace = TemporaryDirectory.Create();
        using var outside = TemporaryDirectory.Create();
        using var secondWorkspace = TemporaryDirectory.Create();
        var writeInsideScript = System.IO.Path.Combine(workspace.Path, "write-inside.cmd");
        var writeOutsideScript = System.IO.Path.Combine(workspace.Path, "write-outside.cmd");
        var insideFile = System.IO.Path.Combine(workspace.Path, "inside.txt");
        var outsideFile = System.IO.Path.Combine(outside.Path, "outside.txt");
        await File.WriteAllTextAsync(writeInsideScript, "@echo off\r\necho allowed>inside.txt\r\n");
        await File.WriteAllTextAsync(
            writeOutsideScript,
            $"@echo off\r\necho blocked>\"{outsideFile}\"\r\n");

        var writePolicy = new ConnectorSandboxSettings(
            true,
            ConnectorSandboxPermissionProfile.WorkspaceWrite,
            ConnectorSandboxNetworkAccess.Disabled);
        var executor = Executor(writePolicy);
        var inside = await executor.ExecuteAsync(Request(writeInsideScript, workspace.Path, "sandbox-write"));
        var outsideResult = await executor.ExecuteAsync(Request(writeOutsideScript, workspace.Path, "sandbox-boundary"));

        Assert.True(inside.Success, inside.Error ?? inside.StandardError);
        Assert.True(File.Exists(insideFile));
        Assert.False(outsideResult.Success);
        Assert.False(File.Exists(outsideFile), outsideResult.Error ?? outsideResult.StandardError);
        Assert.Equal(ConnectorSandboxNetworkAccess.Disabled.ToString(), inside.SandboxNetwork);

        var secondWriteScript = System.IO.Path.Combine(secondWorkspace.Path, "prepare-second.cmd");
        await File.WriteAllTextAsync(secondWriteScript, "@echo off\r\necho second>second.txt\r\n");
        var secondResult = await executor.ExecuteAsync(
            Request(secondWriteScript, secondWorkspace.Path, "sandbox-second-workspace"));
        Assert.True(secondResult.Success, secondResult.Error ?? secondResult.StandardError);
        var crossWorkspaceFile = System.IO.Path.Combine(secondWorkspace.Path, "cross-workspace.txt");
        var crossWorkspaceScript = System.IO.Path.Combine(workspace.Path, "write-cross-workspace.cmd");
        await File.WriteAllTextAsync(
            crossWorkspaceScript,
            $"@echo off\r\necho blocked>\"{crossWorkspaceFile}\"\r\n");
        var crossWorkspace = await executor.ExecuteAsync(
            Request(crossWorkspaceScript, workspace.Path, "sandbox-cross-workspace"));
        Assert.False(crossWorkspace.Success);
        Assert.False(File.Exists(crossWorkspaceFile));

        var nativePolicy = SandboxExecutionPolicy.FromSettings(writePolicy);
        using var context = await WindowsAppContainerSandbox.PrepareAsync(
            workspace.Path,
            nativePolicy,
            CancellationToken.None);
        var capabilities = Marshal.PtrToStructure<SecurityCapabilities>(context.SecurityCapabilities);
        Assert.Equal(0u, capabilities.CapabilityCount);

        var readOnlyScript = System.IO.Path.Combine(workspace.Path, "write-readonly.cmd");
        var readOnlyFile = System.IO.Path.Combine(workspace.Path, "readonly.txt");
        await File.WriteAllTextAsync(readOnlyScript, "@echo off\r\necho denied>readonly.txt\r\n");
        var readOnly = await Executor(new ConnectorSandboxSettings(
            true,
            ConnectorSandboxPermissionProfile.ReadOnly,
            ConnectorSandboxNetworkAccess.Disabled)).ExecuteAsync(
                Request(readOnlyScript, workspace.Path, "sandbox-readonly"));

        Assert.False(readOnly.Success);
        Assert.False(File.Exists(readOnlyFile));
    }

    [Fact]
    public async Task ConPtyAcceptsInputAndProducesOutputOnWindows()
    {
        if (!OperatingSystem.IsWindowsVersionAtLeast(10, 0, 17763)) return;

        using var workspace = TemporaryDirectory.Create();
        await using ITerminalSession session = ConPtyTerminalSession.Start(
            new TerminalSessionIdentity(
                "native-conpty",
                "workspace",
                workspace.Path,
                workspace.Path),
            new TerminalSize(100, 30),
            CommandInterpreter(),
            [],
            sandbox: null);
        var architecture = Environment.GetEnvironmentVariable("PROCESSOR_ARCHITECTURE") ?? "UNKNOWN";
        var expected = $"CHATOS_{architecture}_CONPTY_OK";
        await session.WriteAsync("echo CHATOS_%PROCESSOR_ARCHITECTURE%_CONPTY_OK\r\nexit\r\n");
        var deadline = DateTimeOffset.UtcNow.AddSeconds(10);
        while (!session.Snapshot().Contains(expected, StringComparison.OrdinalIgnoreCase) &&
               DateTimeOffset.UtcNow < deadline)
        {
            await Task.Delay(100);
        }

        Assert.Contains(expected, session.Snapshot(), StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public async Task ControlledCommandAcquiresLeaseBeforeSuspendedProcessCanRunOnWindows()
    {
        if (!OperatingSystem.IsWindows()) return;

        using var workspace = TemporaryDirectory.Create();
        var marker = System.IO.Path.Combine(workspace.Path, "controlled-started.txt");
        var script = System.IO.Path.Combine(workspace.Path, "controlled.cmd");
        await File.WriteAllTextAsync(script, "@echo off\r\necho started>controlled-started.txt\r\n");
        var guard = new AcceptanceNetworkGuardClient(marker);
        var executor = new WindowsTerminalCommandExecutor(
            new SandboxExecutionPolicyProvider(
                new FixedSandboxStore(new ConnectorSandboxSettings(
                    true,
                    ConnectorSandboxPermissionProfile.WorkspaceWrite,
                    ConnectorSandboxNetworkAccess.Controlled)),
                guard),
            new NetworkGuardLeaseCoordinator(guard, maximumRenewInterval: TimeSpan.FromMinutes(1)));
        var policy = new ControlledNetworkPolicyEnvelope(
            "policy-native-1",
            "owner-1",
            "device-1",
            "workspace-controlled",
            "S-1-5-21-100-200-300-400",
            ["api.example.com"],
            [443],
            DateTimeOffset.UtcNow.AddHours(1),
            "key-1",
            "ed25519",
            "test-signature");

        var result = await executor.ExecuteAsync(new TerminalCommandRequest(
            script,
            [],
            workspace.Path,
            workspace.Path,
            "workspace-controlled",
            15_000,
            policy));

        Assert.True(result.Success, result.Error ?? result.StandardError);
        Assert.True(guard.MarkerWasAbsentAtAcquire);
        Assert.Equal(1, guard.AcquireCount);
        Assert.Equal(1, guard.ReleaseCount);
        Assert.True(File.Exists(marker));
    }

    [Fact]
    public async Task ControlledConPtyAcquiresLeaseBeforeSuspendedShellCanRunOnWindows()
    {
        if (!OperatingSystem.IsWindowsVersionAtLeast(10, 0, 17763)) return;

        using var workspace = TemporaryDirectory.Create();
        var marker = System.IO.Path.Combine(workspace.Path, "controlled-conpty-started.txt");
        var guard = new AcceptanceNetworkGuardClient(marker);
        var coordinator = new NetworkGuardLeaseCoordinator(
            guard,
            maximumRenewInterval: TimeSpan.FromMinutes(1));
        var policy = ControlledPolicy("policy-native-conpty", "workspace-controlled-conpty");
        var sandboxPolicy = SandboxExecutionPolicy.FromSettings(new ConnectorSandboxSettings(
            true,
            ConnectorSandboxPermissionProfile.WorkspaceWrite,
            ConnectorSandboxNetworkAccess.Controlled));
        var profileName = WindowsAppContainerSandbox.ProfileName(
            workspace.Path,
            sandboxPolicy.PermissionProfile,
            policy.PolicyRevision);

        await using (var sandbox = await WindowsAppContainerSandbox.PrepareAsync(
            workspace.Path,
            sandboxPolicy,
            policy.PolicyRevision,
            CancellationToken.None))
        {
            await using var session = await ConPtyTerminalSession.StartAsync(
                new TerminalSessionIdentity(
                    "native-controlled-conpty",
                    policy.WorkspaceId,
                    workspace.Path,
                    workspace.Path,
                    policy),
                new TerminalSize(100, 30),
                CommandInterpreter(),
                ["/d", "/s", "/c", $"echo started>\"{marker}\" & ping 127.0.0.1 -n 30 > nul"],
                sandbox,
                coordinator,
                CancellationToken.None);

            var deadline = DateTimeOffset.UtcNow.AddSeconds(5);
            while (!File.Exists(marker) && DateTimeOffset.UtcNow < deadline)
            {
                await Task.Delay(50);
            }
            Assert.True(File.Exists(marker));
            Assert.True(guard.MarkerWasAbsentAtAcquire);
            Assert.Equal(1, guard.AcquireCount);
        }

        Assert.Equal(1, guard.ReleaseCount);
        Assert.False(WindowsAppContainerSandbox.HasPendingProfileCleanup(profileName));
    }

    [Fact]
    public async Task ControlledConPtyAcquireFailureNeverResumesProcessOnWindows()
    {
        if (!OperatingSystem.IsWindowsVersionAtLeast(10, 0, 17763)) return;

        using var workspace = TemporaryDirectory.Create();
        var marker = System.IO.Path.Combine(workspace.Path, "must-not-run.txt");
        var guard = new AcceptanceNetworkGuardClient(marker) { ThrowOnAcquire = true };
        var coordinator = new NetworkGuardLeaseCoordinator(guard);
        var policy = ControlledPolicy("policy-native-conpty-failure", "workspace-controlled-failure");
        var sandboxPolicy = SandboxExecutionPolicy.FromSettings(new ConnectorSandboxSettings(
            true,
            ConnectorSandboxPermissionProfile.WorkspaceWrite,
            ConnectorSandboxNetworkAccess.Controlled));

        await using var sandbox = await WindowsAppContainerSandbox.PrepareAsync(
            workspace.Path,
            sandboxPolicy,
            policy.PolicyRevision,
            CancellationToken.None);
        await Assert.ThrowsAsync<IOException>(() => ConPtyTerminalSession.StartAsync(
            new TerminalSessionIdentity(
                "native-controlled-conpty-failure",
                policy.WorkspaceId,
                workspace.Path,
                workspace.Path,
                policy),
            new TerminalSize(100, 30),
            CommandInterpreter(),
            ["/d", "/s", "/c", $"echo escaped>\"{marker}\""],
            sandbox,
            coordinator,
            CancellationToken.None));

        await Task.Delay(500);
        Assert.False(File.Exists(marker), "The suspended process ran after lease acquisition failed.");
        Assert.Equal(1, guard.AcquireCount);
        Assert.Equal(0, guard.ReleaseCount);
    }

    [Fact]
    public async Task ControlledAppContainerProfileAndWorkspaceAclAreRemovedAfterUseOnWindows()
    {
        if (!OperatingSystem.IsWindows()) return;

        using var workspace = TemporaryDirectory.Create();
        var policy = SandboxExecutionPolicy.FromSettings(new ConnectorSandboxSettings(
            true,
            ConnectorSandboxPermissionProfile.WorkspaceWrite,
            ConnectorSandboxNetworkAccess.Controlled));
        var revision = $"cleanup-{Guid.NewGuid():N}";
        var profileName = WindowsAppContainerSandbox.ProfileName(
            workspace.Path,
            policy.PermissionProfile,
            revision);
        string sid;
        await using (var context = await WindowsAppContainerSandbox.PrepareAsync(
            workspace.Path,
            policy,
            revision,
            CancellationToken.None))
        {
            sid = context.AppContainerSid;
            Assert.True(WindowsAppContainerSandbox.HasPendingProfileCleanup(profileName));
        }

        Assert.False(WindowsAppContainerSandbox.HasPendingProfileCleanup(profileName));
        var acl = await ReadWorkspaceAclAsync(workspace.Path);
        Assert.DoesNotContain(sid, acl, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public async Task CredentialManagerRoundTripsAndDeletesRandomSecretOnWindows()
    {
        if (!OperatingSystem.IsWindows()) return;

        var store = new WindowsCredentialConnectorSecretStore();
        var key = $"native-acceptance-{Guid.NewGuid():N}";
        var secret = $"secret-{Guid.NewGuid():N}";
        try
        {
            await store.SetAsync(key, secret);
            Assert.Equal(secret, await store.GetAsync(key));
        }
        finally
        {
            await store.DeleteAsync(key);
        }

        Assert.Null(await store.GetAsync(key));
    }

    private static WindowsTerminalCommandExecutor Executor(ConnectorSandboxSettings settings) =>
        new(new SandboxExecutionPolicyProvider(new FixedSandboxStore(settings)));

    private static TerminalCommandRequest Request(string command, string workspace, string id) => new(
        command,
        [],
        workspace,
        workspace,
        id,
        15_000);

    private static string CommandInterpreter() =>
        Environment.GetEnvironmentVariable("ComSpec")
        ?? System.IO.Path.Combine(Environment.SystemDirectory, "cmd.exe");

    private static ControlledNetworkPolicyEnvelope ControlledPolicy(
        string revision,
        string workspaceId) => new(
        revision,
        "owner-1",
        "device-1",
        workspaceId,
        "S-1-5-21-100-200-300-400",
        ["api.example.com"],
        [443],
        DateTimeOffset.UtcNow.AddHours(1),
        "key-1",
        "ed25519",
        "test-signature");

    private static async Task<string> ReadWorkspaceAclAsync(string workspaceRoot)
    {
        var start = new ProcessStartInfo
        {
            FileName = System.IO.Path.Combine(Environment.SystemDirectory, "icacls.exe"),
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };
        start.ArgumentList.Add(workspaceRoot);
        using var process = Process.Start(start)
            ?? throw new InvalidOperationException("Unable to inspect workspace ACL.");
        var output = await process.StandardOutput.ReadToEndAsync();
        var error = await process.StandardError.ReadToEndAsync();
        await process.WaitForExitAsync();
        Assert.True(process.ExitCode == 0, error);
        return output;
    }

    private sealed class FixedSandboxStore(ConnectorSandboxSettings settings)
        : IConnectorSandboxSettingsStore
    {
        public Task<ConnectorSandboxSettings> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult(settings);

        public Task SaveAsync(
            ConnectorSandboxSettings value,
            CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();
    }

    private sealed class AcceptanceNetworkGuardClient(string markerPath) : IControlledNetworkGuardClient
    {
        public bool ThrowOnAcquire { get; init; }
        public int AcquireCount { get; private set; }
        public int ReleaseCount { get; private set; }
        public bool MarkerWasAbsentAtAcquire { get; private set; }

        public Task<NetworkGuardReadiness> CheckReadinessAsync(
            CancellationToken cancellationToken = default) =>
            Task.FromResult(new NetworkGuardReadiness(
                NetworkGuardReadinessState.Ready,
                "test-service",
                "test-driver"));

        public Task<NetworkGuardLease> AcquireLeaseAsync(
            ControlledNetworkPolicyEnvelope policy,
            string appContainerSid,
            int processId,
            CancellationToken cancellationToken = default)
        {
            AcquireCount++;
            MarkerWasAbsentAtAcquire = !File.Exists(markerPath);
            Assert.False(Process.GetProcessById(processId).HasExited);
            if (ThrowOnAcquire)
            {
                throw new IOException("simulated NetworkGuard acquisition failure");
            }
            return Task.FromResult(new NetworkGuardLease(
                "lease-native-1",
                DateTimeOffset.UtcNow.AddMinutes(5),
                policy.PolicyRevision,
                appContainerSid,
                processId));
        }

        public Task<NetworkGuardLease> RenewLeaseAsync(
            NetworkGuardLease lease,
            CancellationToken cancellationToken = default) =>
            Task.FromResult(lease with { ExpiresAt = DateTimeOffset.UtcNow.AddMinutes(5) });

        public Task ReleaseLeaseAsync(
            NetworkGuardLease lease,
            CancellationToken cancellationToken = default)
        {
            ReleaseCount++;
            return Task.CompletedTask;
        }
    }

    private sealed class TemporaryDirectory : IDisposable
    {
        private TemporaryDirectory(string path)
        {
            Path = path;
        }

        public string Path { get; }

        public static TemporaryDirectory Create()
        {
            var path = System.IO.Path.Combine(
                System.IO.Path.GetTempPath(),
                $"chatos-native-{Guid.NewGuid():N}");
            Directory.CreateDirectory(path);
            return new TemporaryDirectory(path);
        }

        public void Dispose()
        {
            for (var attempt = 0; attempt < 20; attempt++)
            {
                if (!Directory.Exists(Path)) return;
                try
                {
                    Directory.Delete(Path, recursive: true);
                    return;
                }
                catch (IOException) when (attempt < 19)
                {
                    Thread.Sleep(100);
                }
                catch (UnauthorizedAccessException) when (attempt < 19)
                {
                    Thread.Sleep(100);
                }
            }
        }
    }
}
