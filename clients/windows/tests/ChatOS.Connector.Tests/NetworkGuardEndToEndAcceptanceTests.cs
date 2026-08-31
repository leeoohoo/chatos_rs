using System.Diagnostics;
using System.Net;
using System.Security.Principal;
using ChatOS.Connector.NetworkGuard;
using ChatOS.Connector.Sandbox;
using ChatOS.Connector.Terminal;
using ChatOS.NetworkGuard.Contracts;
using Org.BouncyCastle.Crypto.Parameters;
using Org.BouncyCastle.Math.EC.Rfc8032;
using Org.BouncyCastle.Security;

namespace ChatOS.Connector.Tests;

[Trait("Category", "NetworkGuardEndToEnd")]
public sealed class NetworkGuardEndToEndAcceptanceTests
{
    private const string ServiceName = "ChatOSNetworkGuardService";
    private const string DriverName = "ChatOSNetworkGuard";

    [Fact]
    public async Task SignedPolicyAllowsOnlyApprovedHttpTlsAndLeavesNoLeaseResidue()
    {
        if (!AcceptanceEnabled()) return;
        RequireWindowsAdministrator();
        using var workspace = TemporaryDirectory.Create();
        var context = AcceptanceContext.Load(workspace.Path);

        var allowedAddresses = await Dns.GetHostAddressesAsync(context.AllowedUri.Host);
        var deniedAddresses = await Dns.GetHostAddressesAsync(context.DeniedUri.Host);
        Assert.NotEmpty(allowedAddresses.Intersect(deniedAddresses));

        var allowed = await context.ExecuteCurlAsync(context.AllowedUri);
        Assert.True(allowed.Success, allowed.Error ?? allowed.StandardError);
        var allowedHttpUri = new UriBuilder(context.AllowedUri)
        {
            Scheme = Uri.UriSchemeHttp,
            Port = 80,
        }.Uri;
        var allowedHttp = await context.ExecuteCurlAsync(allowedHttpUri);
        Assert.True(allowedHttp.Success, allowedHttp.Error ?? allowedHttp.StandardError);

        var denied = await context.ExecuteCurlAsync(context.DeniedUri);
        Assert.False(denied.Success, "A denied TLS SNI host sharing an approved IP was reachable.");

        var deniedDoh = await context.ExecuteCurlAsync(
            new Uri(context.DeniedUri, "/dns-query?dns=AAABAAABAAAAAAAAA3d3dwdleGFtcGxlA2NvbQAAAQAB"));
        Assert.False(deniedDoh.Success, "A denied DNS-over-HTTPS endpoint was reachable.");

        var ipv4 = allowedAddresses.FirstOrDefault(value => value.AddressFamily == System.Net.Sockets.AddressFamily.InterNetwork);
        Assert.NotNull(ipv4);
        var literal = await context.ExecuteCurlAsync(new Uri($"https://{ipv4}/"));
        Assert.False(literal.Success, "An IPv4 literal bypassed the signed hostname policy.");

        var ipv6Text = Environment.GetEnvironmentVariable("CHATOS_NETWORKGUARD_ACCEPTANCE_IPV6_LITERAL");
        if (!string.IsNullOrWhiteSpace(ipv6Text))
        {
            var ipv6 = IPAddress.Parse(ipv6Text.Trim());
            Assert.Equal(System.Net.Sockets.AddressFamily.InterNetworkV6, ipv6.AddressFamily);
            var ipv6Literal = await context.ExecuteCurlAsync(new Uri($"https://[{ipv6}]/"));
            Assert.False(ipv6Literal.Success, "An IPv6 literal bypassed the signed hostname policy.");
        }

        Assert.True(await context.ExecuteUdpMustBeBlockedAsync(context.AllowedUri.Host, 443));
        Assert.True(await context.ExecuteUdpMustBeBlockedAsync("1.1.1.1", 53));
        Assert.True(await context.ExecuteNoSniMustBeBlockedAsync());
        Assert.True(await context.ExecuteHostFromChildAsync(context.AllowedUri));
        Assert.False(await context.ExecuteHostFromChildAsync(context.DeniedUri));

        await context.AssertNoLeaseResidueAsync();
    }

    [Fact]
    public async Task ServiceAndDriverRestartRemainFailClosedAndReconcileResidue()
    {
        if (!AcceptanceEnabled() ||
            !string.Equals(
                Environment.GetEnvironmentVariable("CHATOS_NETWORKGUARD_ACCEPTANCE_DISRUPTIVE"),
                "1",
                StringComparison.Ordinal))
        {
            return;
        }
        RequireWindowsAdministrator();
        using var workspace = TemporaryDirectory.Create();
        var context = AcceptanceContext.Load(workspace.Path, renewalInterval: TimeSpan.FromMilliseconds(250));

        await AssertRestartFailsClosedAsync(context, ServiceName);
        await AssertRestartFailsClosedAsync(context, DriverName);
        await context.AssertNoLeaseResidueAsync();
    }

    private static async Task AssertRestartFailsClosedAsync(
        AcceptanceContext context,
        string serviceName)
    {
        var prefix = serviceName == DriverName ? "driver" : "service";
        var started = Path.Combine(context.WorkspaceRoot, $"{prefix}-started.txt");
        var escaped = Path.Combine(context.WorkspaceRoot, $"{prefix}-escaped.txt");
        var script = Path.Combine(context.WorkspaceRoot, $"{prefix}-restart.cmd");
        await File.WriteAllTextAsync(script, $$"""
            @echo off
            echo started>"{{started}}"
            ping 127.0.0.1 -n 12 > nul
            echo escaped>"{{escaped}}"
            """);

        var execution = context.ExecuteAsync(script, [], timeoutMilliseconds: 30_000);
        var deadline = DateTimeOffset.UtcNow.AddSeconds(8);
        while (!File.Exists(started) && DateTimeOffset.UtcNow < deadline)
        {
            await Task.Delay(50);
        }
        Assert.True(File.Exists(started), "The controlled process never started before disruption.");

        try
        {
            if (serviceName == DriverName)
            {
                await RunScAsync("stop", ServiceName);
                await WaitForServiceStateAsync(ServiceName, 1, TimeSpan.FromSeconds(15));
            }
            await RunScAsync("stop", serviceName);
            await WaitForServiceStateAsync(serviceName, 1, TimeSpan.FromSeconds(15));
            var result = await execution.WaitAsync(TimeSpan.FromSeconds(12));
            Assert.False(result.Success, "The controlled process survived NetworkGuard disruption.");
            Assert.False(File.Exists(escaped), "The process tree escaped after NetworkGuard disruption.");
        }
        finally
        {
            await RunScAsync("start", serviceName, allowAlreadyRunning: true);
            await WaitForServiceStateAsync(serviceName, 4, TimeSpan.FromSeconds(15));
            if (serviceName == DriverName)
            {
                await RunScAsync("start", ServiceName, allowAlreadyRunning: true);
                await WaitForServiceStateAsync(ServiceName, 4, TimeSpan.FromSeconds(15));
            }
            await context.WaitUntilReadyAsync();
        }
    }

    private static async Task WaitForServiceStateAsync(
        string serviceName,
        int expectedState,
        TimeSpan timeout)
    {
        var deadline = DateTimeOffset.UtcNow + timeout;
        while (DateTimeOffset.UtcNow < deadline)
        {
            var start = new ProcessStartInfo
            {
                FileName = WindowsPowerShellPath(),
                UseShellExecute = false,
                CreateNoWindow = true,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
            };
            start.ArgumentList.Add("-NoLogo");
            start.ArgumentList.Add("-NoProfile");
            start.ArgumentList.Add("-NonInteractive");
            start.ArgumentList.Add("-Command");
            start.ArgumentList.Add($"[int](Get-Service -Name '{serviceName}').Status");
            using var process = Process.Start(start)
                ?? throw new InvalidOperationException("Unable to query Windows service state.");
            var output = await process.StandardOutput.ReadToEndAsync();
            await process.WaitForExitAsync();
            if (process.ExitCode == 0 &&
                int.TryParse(output.Trim(), out var state) &&
                state == expectedState)
            {
                return;
            }
            await Task.Delay(250);
        }
        throw new TimeoutException($"{serviceName} did not reach service state {expectedState}.");
    }

    private static async Task RunScAsync(
        string action,
        string serviceName,
        bool allowAlreadyRunning = false)
    {
        var start = new ProcessStartInfo
        {
            FileName = "sc.exe",
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };
        start.ArgumentList.Add(action);
        start.ArgumentList.Add(serviceName);
        using var process = Process.Start(start)
            ?? throw new InvalidOperationException("Unable to run sc.exe.");
        var output = await process.StandardOutput.ReadToEndAsync();
        var error = await process.StandardError.ReadToEndAsync();
        await process.WaitForExitAsync();
        if (process.ExitCode != 0 &&
            !(allowAlreadyRunning && (output.Contains("1056", StringComparison.Ordinal) ||
                                      error.Contains("1056", StringComparison.Ordinal))))
        {
            throw new InvalidOperationException($"sc.exe {action} failed with exit code {process.ExitCode}.");
        }
    }

    private static bool AcceptanceEnabled() => OperatingSystem.IsWindows() &&
        string.Equals(
            Environment.GetEnvironmentVariable("CHATOS_NETWORKGUARD_END_TO_END"),
            "1",
            StringComparison.Ordinal);

    private static string WindowsPowerShellPath() => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.Windows),
        "System32",
        "WindowsPowerShell",
        "v1.0",
        "powershell.exe");

    private static void RequireWindowsAdministrator()
    {
        var principal = new WindowsPrincipal(WindowsIdentity.GetCurrent());
        Assert.True(
            principal.IsInRole(WindowsBuiltInRole.Administrator),
            "NetworkGuard end-to-end acceptance must run elevated.");
    }

    private sealed class AcceptanceContext
    {
        private readonly ControlledNetworkGuardClient _client;
        private readonly WindowsTerminalCommandExecutor _executor;
        private readonly ControlledNetworkPolicyEnvelope _policy;

        private AcceptanceContext(
            string workspaceRoot,
            Uri allowedUri,
            Uri deniedUri,
            ControlledNetworkGuardClient client,
            WindowsTerminalCommandExecutor executor,
            ControlledNetworkPolicyEnvelope policy)
        {
            WorkspaceRoot = workspaceRoot;
            AllowedUri = allowedUri;
            DeniedUri = deniedUri;
            _client = client;
            _executor = executor;
            _policy = policy;
        }

        public string WorkspaceRoot { get; }

        public Uri AllowedUri { get; }

        public Uri DeniedUri { get; }

        public static AcceptanceContext Load(
            string workspaceRoot,
            TimeSpan? renewalInterval = null)
        {
            var keyPath = RequiredEnvironment("CHATOS_NETWORKGUARD_ACCEPTANCE_PRIVATE_KEY_PATH");
            var keyId = RequiredEnvironment("CHATOS_NETWORKGUARD_ACCEPTANCE_KEY_ID");
            var allowedUri = HttpsUri(RequiredEnvironment("CHATOS_NETWORKGUARD_ACCEPTANCE_ALLOWED_URL"));
            var deniedUri = HttpsUri(RequiredEnvironment("CHATOS_NETWORKGUARD_ACCEPTANCE_DENIED_URL"));
            Assert.NotEqual(allowedUri.Host, deniedUri.Host);

            var privateKey = PrivateKeyFactory.CreateKey(File.ReadAllBytes(keyPath))
                as Ed25519PrivateKeyParameters
                ?? throw new InvalidOperationException("Acceptance key is not an Ed25519 PKCS#8 private key.");
            var windowsSid = WindowsIdentity.GetCurrent().User?.Value
                ?? throw new InvalidOperationException("The current Windows SID is unavailable.");
            var unsigned = new ControlledNetworkPolicy(
                Guid.NewGuid().ToString("N"),
                "networkguard-acceptance",
                Environment.MachineName,
                "networkguard-acceptance-workspace",
                windowsSid,
                [allowedUri.IdnHost.ToLowerInvariant()],
                [80, 443],
                DateTimeOffset.UtcNow.AddMinutes(10),
                keyId);
            var signature = new byte[Ed25519PrivateKeyParameters.SignatureSize];
            privateKey.Sign(
                Ed25519.Algorithm.Ed25519,
                null,
                ControlledNetworkPolicyValidator.SignaturePayload(unsigned),
                signature);
            var envelope = new ControlledNetworkPolicyEnvelope(
                unsigned.PolicyRevision,
                unsigned.OwnerUserId,
                unsigned.DeviceId,
                unsigned.WorkspaceId,
                unsigned.WindowsUserSid,
                unsigned.AllowedHosts,
                unsigned.AllowedPorts,
                unsigned.ExpiresAt,
                unsigned.SignatureKeyId,
                "ed25519",
                Base64Url(signature));
            var client = new ControlledNetworkGuardClient(new NamedPipeNetworkGuardTransport());
            var coordinator = new NetworkGuardLeaseCoordinator(
                client,
                maximumRenewInterval: renewalInterval ?? TimeSpan.FromSeconds(2));
            var settings = new ConnectorSandboxSettings(
                true,
                ConnectorSandboxPermissionProfile.WorkspaceWrite,
                ConnectorSandboxNetworkAccess.Controlled);
            var executor = new WindowsTerminalCommandExecutor(
                new SandboxExecutionPolicyProvider(new FixedSandboxStore(settings), client),
                coordinator);
            return new AcceptanceContext(
                workspaceRoot,
                allowedUri,
                deniedUri,
                client,
                executor,
                envelope);
        }

        public Task<TerminalCommandResult> ExecuteCurlAsync(Uri uri) => ExecuteAsync(
            "curl.exe",
            ["--insecure", "--silent", "--show-error", "--connect-timeout", "4", "--max-time", "8", uri.AbsoluteUri],
            timeoutMilliseconds: 12_000);

        public Task<TerminalCommandResult> ExecuteAsync(
            string command,
            IReadOnlyList<string> arguments,
            int timeoutMilliseconds) => _executor.ExecuteAsync(new TerminalCommandRequest(
            command,
            arguments,
            WorkspaceRoot,
            WorkspaceRoot,
            _policy.WorkspaceId,
            timeoutMilliseconds,
            _policy));

        public async Task<bool> ExecuteUdpMustBeBlockedAsync(string host, int port)
        {
            var script = Path.Combine(WorkspaceRoot, $"udp-{Guid.NewGuid():N}.ps1");
            await File.WriteAllTextAsync(script, $$"""
                $ErrorActionPreference = 'Stop'
                try {
                  $client = [Net.Sockets.UdpClient]::new()
                  $client.Connect('{{host}}', {{port}})
                  $bytes = [byte[]](1,2,3,4)
                  [void]$client.Send($bytes, $bytes.Length)
                  exit 9
                } catch {
                  exit 0
                } finally {
                  if ($null -ne $client) { $client.Dispose() }
                }
                """);
            var result = await ExecuteAsync(
                WindowsPowerShellPath(),
                ["-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", script],
                10_000);
            return result.Success;
        }

        public async Task<bool> ExecuteNoSniMustBeBlockedAsync()
        {
            var script = Path.Combine(WorkspaceRoot, $"no-sni-{Guid.NewGuid():N}.ps1");
            await File.WriteAllTextAsync(script, $$"""
                $ErrorActionPreference = 'Stop'
                try {
                  $tcp = [Net.Sockets.TcpClient]::new()
                  $tcp.Connect('{{AllowedUri.Host}}', 443)
                  $ssl = [Net.Security.SslStream]::new($tcp.GetStream(), $false, { $true })
                  $ssl.AuthenticateAsClient('127.0.0.1')
                  exit 9
                } catch {
                  exit 0
                } finally {
                  if ($null -ne $ssl) { $ssl.Dispose() }
                  if ($null -ne $tcp) { $tcp.Dispose() }
                }
                """);
            var result = await ExecuteAsync(
                WindowsPowerShellPath(),
                ["-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", script],
                12_000);
            return result.Success;
        }

        public async Task<bool> ExecuteHostFromChildAsync(Uri uri)
        {
            var script = Path.Combine(WorkspaceRoot, $"child-{Guid.NewGuid():N}.ps1");
            await File.WriteAllTextAsync(script, $$"""
                $arguments = @('--insecure','--silent','--show-error','--connect-timeout','4','--max-time','8','{{uri.AbsoluteUri}}')
                $child = Start-Process -FilePath 'curl.exe' -ArgumentList $arguments -NoNewWindow -Wait -PassThru
                exit $child.ExitCode
                """);
            var result = await ExecuteAsync(
                WindowsPowerShellPath(),
                ["-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", script],
                12_000);
            return result.Success;
        }

        public async Task WaitUntilReadyAsync()
        {
            var deadline = DateTimeOffset.UtcNow.AddSeconds(20);
            while (DateTimeOffset.UtcNow < deadline)
            {
                var readiness = await _client.CheckReadinessAsync();
                if (readiness.IsReady) return;
                await Task.Delay(250);
            }
            throw new TimeoutException("NetworkGuard did not become ready after restart.");
        }

        public async Task AssertNoLeaseResidueAsync()
        {
            var deadline = DateTimeOffset.UtcNow.AddSeconds(10);
            NetworkGuardReadiness readiness;
            do
            {
                readiness = await _client.CheckReadinessAsync();
                if (readiness.IsReady && readiness.ActiveLeaseCount == 0) return;
                await Task.Delay(100);
            }
            while (DateTimeOffset.UtcNow < deadline);
            Assert.True(readiness.IsReady, $"NetworkGuard is not ready: {readiness.State}");
            Assert.Equal(0, readiness.ActiveLeaseCount);
        }

        private static string RequiredEnvironment(string name) =>
            Environment.GetEnvironmentVariable(name)?.Trim() is { Length: > 0 } value
                ? value
                : throw new InvalidOperationException($"{name} is required for NetworkGuard acceptance.");

        private static Uri HttpsUri(string value)
        {
            var uri = new Uri(value, UriKind.Absolute);
            if (!string.Equals(uri.Scheme, Uri.UriSchemeHttps, StringComparison.OrdinalIgnoreCase) ||
                uri.Port != 443 || IPAddress.TryParse(uri.Host, out _))
            {
                throw new InvalidOperationException("Acceptance URLs must use HTTPS hostnames on port 443.");
            }
            return uri;
        }

        private static string Base64Url(byte[] value) => Convert.ToBase64String(value)
            .TrimEnd('=')
            .Replace('+', '-')
            .Replace('/', '_');
    }

    private sealed class FixedSandboxStore(ConnectorSandboxSettings settings)
        : IConnectorSandboxSettingsStore
    {
        public Task<ConnectorSandboxSettings> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult(settings);

        public Task SaveAsync(
            ConnectorSandboxSettings value,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
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
                $"chatos-networkguard-e2e-{Guid.NewGuid():N}");
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
