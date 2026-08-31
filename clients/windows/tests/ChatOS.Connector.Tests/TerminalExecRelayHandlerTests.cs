using ChatOS.Connector.Approval;
using ChatOS.Connector.Relay;
using ChatOS.Connector.Terminal;
using ChatOS.Connector.Workspaces;

namespace ChatOS.Connector.Tests;

public sealed class TerminalExecRelayHandlerTests
{
    [Fact]
    public async Task ApprovedCommandExecutesAndReturnsCompleteAuditShape()
    {
        using var workspace = TestWorkspace.Create();
        var approvals = new CommandApprovalCoordinator(
            new CommandApprovalCoordinatorTests.MemoryApprovalStore());
        var executor = new FakeExecutor();
        var history = new FakeHistoryStore();
        var dispatcher = Dispatcher(workspace, approvals, executor, history);
        var responseTask = dispatcher.DispatchAsync(Payload(workspace.Root));
        await WaitUntilAsync(() => approvals.Snapshot().Count == 1);
        Assert.True(await approvals.ResolveAsync(
            approvals.Snapshot()[0].Id,
            ConnectorApprovalAction.Accept));

        var response = await responseTask;
        Assert.Equal(200, response.Status);
        Assert.Equal("terminal_response", response.Type);
        Assert.Equal("approved", response.Body.GetProperty("approval_decision").GetString());
        Assert.Equal("hello", response.Body.GetProperty("stdout").GetString());
        Assert.Equal(5, response.Body.GetProperty("stdout_bytes").GetInt64());
        Assert.True(response.Body.GetProperty("audit_persisted").GetBoolean());
        Assert.Single(history.Entries);
        Assert.Equal(workspace.Root, executor.Request?.WorkingDirectory);
    }

    [Fact]
    public async Task DeclinedCommandNeverReachesExecutor()
    {
        using var workspace = TestWorkspace.Create();
        var approvals = new CommandApprovalCoordinator(
            new CommandApprovalCoordinatorTests.MemoryApprovalStore());
        var executor = new FakeExecutor();
        var dispatcher = Dispatcher(workspace, approvals, executor);
        var responseTask = dispatcher.DispatchAsync(Payload(workspace.Root));
        await WaitUntilAsync(() => approvals.Snapshot().Count == 1);
        await approvals.ResolveAsync(
            approvals.Snapshot()[0].Id,
            ConnectorApprovalAction.Decline);

        var response = await responseTask;
        Assert.Equal(200, response.Status);
        Assert.Equal("denied", response.Body.GetProperty("approval_decision").GetString());
        Assert.Null(executor.Request);
    }

    [Fact]
    public async Task TimeoutUsesRelayStatus408()
    {
        using var workspace = TestWorkspace.Create();
        var approvals = new CommandApprovalCoordinator(
            new CommandApprovalCoordinatorTests.MemoryApprovalStore());
        await approvals.SetModeAsync(
            ConnectorApprovalMode.FullControl,
            fullControlRiskConfirmed: true);
        var executor = new FakeExecutor { TimedOut = true };
        var response = await Dispatcher(workspace, approvals, executor)
            .DispatchAsync(Payload(workspace.Root));

        Assert.Equal(408, response.Status);
        Assert.True(response.Body.GetProperty("timed_out").GetBoolean());
    }

    [Fact]
    public async Task WorkingDirectoryCannotEscapeWorkspace()
    {
        using var workspace = TestWorkspace.Create();
        var approvals = new CommandApprovalCoordinator(
            new CommandApprovalCoordinatorTests.MemoryApprovalStore());
        var payload = Payload(workspace.Root).Replace(
            "\"cwd\": \".\"",
            "\"cwd\": \"../outside\"");
        var response = await Dispatcher(workspace, approvals, new FakeExecutor())
            .DispatchAsync(payload);

        Assert.Equal(400, response.Status);
        Assert.Empty(approvals.Snapshot());
    }

    [Fact]
    public async Task ControlledPolicyIsPassedToExecutorOnlyWhenRelayIdentityMatches()
    {
        using var workspace = TestWorkspace.Create();
        var approvals = new CommandApprovalCoordinator(
            new CommandApprovalCoordinatorTests.MemoryApprovalStore());
        await approvals.SetModeAsync(
            ConnectorApprovalMode.FullControl,
            fullControlRiskConfirmed: true);
        var executor = new FakeExecutor();

        var response = await Dispatcher(workspace, approvals, executor)
            .DispatchAsync(PayloadWithPolicy(workspace.Root, "workspace-1"));

        Assert.Equal(200, response.Status);
        Assert.NotNull(executor.Request?.NetworkPolicy);
        Assert.Equal("policy-1", executor.Request.NetworkPolicy.PolicyRevision);
    }

    [Fact]
    public async Task ControlledPolicyIdentityMismatchIsRejectedBeforeApproval()
    {
        using var workspace = TestWorkspace.Create();
        var approvals = new CommandApprovalCoordinator(
            new CommandApprovalCoordinatorTests.MemoryApprovalStore());
        var executor = new FakeExecutor();

        var response = await Dispatcher(workspace, approvals, executor)
            .DispatchAsync(PayloadWithPolicy(workspace.Root, "other-workspace"));

        Assert.Equal(400, response.Status);
        Assert.Empty(approvals.Snapshot());
        Assert.Null(executor.Request);
    }

    [Fact]
    public void WindowsCommandLineQuotingPreservesSpacesQuotesAndTrailingSlashes()
    {
        var value = WindowsTerminalCommandExecutor.BuildCommandLine(
            "tool.exe",
            ["plain", "two words", "quote\"value", "C:\\path with space\\"]);

        Assert.Equal(
            "tool.exe plain \"two words\" \"quote\\\"value\" \"C:\\path with space\\\\\"",
            value);
    }

    private static RelayDispatcher Dispatcher(
        TestWorkspace workspace,
        CommandApprovalCoordinator approvals,
        ITerminalCommandExecutor executor,
        ITerminalCommandHistoryStore? history = null)
    {
        var sessions = new TerminalSessionManager(new UnsupportedSessionFactory());
        var handler = new TerminalRelayHandler(
            workspace,
            sessions,
            new ConnectorOutboundEventHub(),
            approvals,
            executor,
            commandHistory: history);
        return new RelayDispatcher([handler], new AcceptingVerifier(), [handler]);
    }

    private static string Payload(string root) => $$"""
        {
          "type": "terminal_exec_request",
          "request_id": "request-exec",
          "owner_user_id": "owner-1",
          "device_id": "device-1",
          "workspace_id": "workspace-1",
          "headers": {},
          "body": {
            "command": "cmd.exe",
            "args": ["/c", "echo", "hello"],
            "cwd": ".",
            "timeout_ms": 30000,
            "source": "test"
          }
        }
        """;

    private static string PayloadWithPolicy(string root, string policyWorkspaceId) =>
        Payload(root).Replace(
            "\"source\": \"test\"",
            $$"""
            "source": "test",
            "network_policy": {
              "policy_revision": "policy-1",
              "owner_user_id": "owner-1",
              "device_id": "device-1",
              "workspace_id": "{{policyWorkspaceId}}",
              "windows_user_sid": "S-1-5-21-100-200-300-400",
              "allowed_hosts": ["api.example.com"],
              "allowed_ports": [443],
              "expires_at": "2099-01-01T00:00:00Z",
              "signature_key_id": "key-1",
              "signature_alg": "ed25519",
              "signature": "test-signature"
            }
            """);

    private static async Task WaitUntilAsync(Func<bool> condition)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(2));
        while (!condition())
        {
            await Task.Delay(5, timeout.Token);
        }
    }

    private sealed class AcceptingVerifier : IRelayRequestVerifier
    {
        public Task VerifyAsync(RelayRequest request, CancellationToken cancellationToken) =>
            Task.CompletedTask;
    }

    private sealed class FakeExecutor : ITerminalCommandExecutor
    {
        public TerminalCommandRequest? Request { get; private set; }

        public bool TimedOut { get; init; }

        public Task<TerminalCommandResult> ExecuteAsync(
            TerminalCommandRequest request,
            CancellationToken cancellationToken = default)
        {
            Request = request;
            return Task.FromResult(new TerminalCommandResult(
                request.Command,
                request.Arguments,
                request.WorkingDirectory,
                request.WorkspaceId,
                Success: !TimedOut,
                ExitCode: TimedOut ? 1460 : 0,
                TimedOut,
                request.TimeoutMilliseconds,
                StandardOutput: "hello",
                StandardError: string.Empty,
                StandardOutputBytes: 5,
                StandardErrorBytes: 0,
                StandardOutputTruncated: false,
                StandardErrorTruncated: false,
                Error: TimedOut ? "timed out" : null));
        }
    }

    private sealed class UnsupportedSessionFactory : ITerminalSessionFactory
    {
        public Task<ITerminalSession> CreateAsync(
            TerminalSessionIdentity identity,
            TerminalSize size,
            CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();
    }

    private sealed class FakeHistoryStore : ITerminalCommandHistoryStore
    {
        public List<TerminalCommandHistoryEntry> Entries { get; } = [];

        public Task AppendAsync(
            TerminalCommandHistoryEntry entry,
            CancellationToken cancellationToken = default)
        {
            Entries.Add(entry);
            return Task.CompletedTask;
        }

        public Task<IReadOnlyList<TerminalCommandHistoryEntry>> ReadAsync(
            int limit = 1_000,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<TerminalCommandHistoryEntry>>(
                Entries.Take(limit).ToArray());
    }

    private sealed class TestWorkspace : IConnectorWorkspaceCatalog, IDisposable
    {
        private TestWorkspace(string root) => Root = root;

        public string Root { get; }

        public static TestWorkspace Create()
        {
            var root = Path.Combine(Path.GetTempPath(), $"chatos-exec-{Guid.NewGuid():N}");
            Directory.CreateDirectory(root);
            return new TestWorkspace(root);
        }

        public ConnectorWorkspace? Find(string workspaceId) => workspaceId == "workspace-1"
            ? new ConnectorWorkspace("workspace-1", "Workspace", Root, "fingerprint")
            : null;

        public void Dispose() => Directory.Delete(Root, recursive: true);
    }
}
