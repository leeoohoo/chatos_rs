using System.Text.Json;
using System.Text.Json.Serialization;
using System.Security.Cryptography;
using System.Text;
using ChatOS.Connector.Approval;
using ChatOS.Connector.Relay;
using ChatOS.Connector.Workspaces;
using ChatOS.NetworkGuard.Contracts;

namespace ChatOS.Connector.Terminal;

public sealed class TerminalRelayHandler : IRelayRequestHandler, IRelayOneWayHandler
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = true,
    };

    private readonly IConnectorWorkspaceCatalog _workspaces;
    private readonly TerminalSessionManager _sessions;
    private readonly ConnectorOutboundEventHub _events;
    private readonly CommandApprovalCoordinator? _approvals;
    private readonly ITerminalCommandExecutor? _commandExecutor;
    private readonly CommandRiskEvaluator _riskEvaluator;
    private readonly ITerminalCommandHistoryStore? _commandHistory;
    private readonly TimeProvider _timeProvider;

    public TerminalRelayHandler(
        IConnectorWorkspaceCatalog workspaces,
        TerminalSessionManager sessions,
        ConnectorOutboundEventHub events,
        CommandApprovalCoordinator? approvals = null,
        ITerminalCommandExecutor? commandExecutor = null,
        CommandRiskEvaluator? riskEvaluator = null,
        ITerminalCommandHistoryStore? commandHistory = null,
        TimeProvider? timeProvider = null)
    {
        _workspaces = workspaces;
        _sessions = sessions;
        _events = events;
        _approvals = approvals;
        _commandExecutor = commandExecutor;
        _riskEvaluator = riskEvaluator ?? new CommandRiskEvaluator();
        _commandHistory = commandHistory;
        _timeProvider = timeProvider ?? TimeProvider.System;
    }

    bool IRelayRequestHandler.CanHandle(string requestType) =>
        requestType is "terminal_session_create_request" or "terminal_exec_request";

    bool IRelayOneWayHandler.CanHandle(string requestType) => requestType is
        "terminal_input" or
        "terminal_command" or
        "terminal_resize" or
        "terminal_snapshot_request" or
        "terminal_close";

    public string ResponseType(string requestType) => requestType == "terminal_exec_request"
        ? "terminal_response"
        : "terminal_session_create_response";

    async Task<RelayHandlerResult> IRelayRequestHandler.HandleAsync(
        RelayRequest request,
        CancellationToken cancellationToken) => request.Type == "terminal_exec_request"
        ? await ExecuteCommandAsync(request, cancellationToken).ConfigureAwait(false)
        : RelayHandlerResult.Ok(
            await CreateSessionAsync(request, cancellationToken).ConfigureAwait(false));

    async Task IRelayOneWayHandler.HandleAsync(
        RelayRequest request,
        CancellationToken cancellationToken)
    {
        var body = Deserialize<TerminalControlBody>(request.Body);
        var sessionId = Required(body.TerminalSessionId, "terminal_session_id");
        var session = await _sessions.GetAsync(sessionId).ConfigureAwait(false);
        if (session is null ||
            !string.Equals(session.Identity.WorkspaceId, request.WorkspaceId, StringComparison.Ordinal))
        {
            _events.Publish(new TerminalEvent(
                TerminalEventKind.Error,
                sessionId,
                Data: "Terminal session was not found for this workspace."));
            return;
        }

        if (request.Type == "terminal_close")
        {
            await _sessions.CloseAsync(sessionId, cancellationToken).ConfigureAwait(false);
            return;
        }

        switch (request.Type)
        {
            case "terminal_input":
                await session.WriteAsync(body.Data ?? string.Empty, cancellationToken)
                    .ConfigureAwait(false);
                break;
            case "terminal_command":
                // Command metadata is consumed by approval/history in the next layer; it is not shell input.
                break;
            case "terminal_resize":
                await session.ResizeAsync(
                    TerminalSize.Normalize(body.Columns ?? 80, body.Rows ?? 24),
                    cancellationToken).ConfigureAwait(false);
                break;
            case "terminal_snapshot_request":
                _events.Publish(new TerminalEvent(
                    TerminalEventKind.Snapshot,
                    sessionId,
                    Data: session.Snapshot(body.Lines ?? 500)));
                break;
        }
    }

    private async Task<JsonElement> CreateSessionAsync(
        RelayRequest request,
        CancellationToken cancellationToken)
    {
        var workspace = _workspaces.Find(request.WorkspaceId)
            ?? throw new RelayRequestException(400, "Terminal workspace is not registered locally.");
        var body = Deserialize<TerminalCreateBody>(request.Body);
        var sessionId = Required(body.TerminalSessionId, "terminal_session_id");
        var paths = new WorkspacePathGuard(workspace.AbsoluteRoot);
        var relativeWorkingDirectory = CombineRelative(
            request.Header("x-local-connector-cwd") ?? ".",
            body.WorkingDirectory ?? ".");
        var workingDirectory = paths.ResolveExisting(relativeWorkingDirectory);
        if (!Directory.Exists(workingDirectory))
        {
            throw new RelayRequestException(400, "Terminal working directory is not a directory.");
        }

        var identity = new TerminalSessionIdentity(
            sessionId,
            workspace.Id,
            paths.Root,
            workingDirectory,
            ValidateControlledPolicyScope(body.NetworkPolicy, request));
        var session = await _sessions.EnsureSessionAsync(
            identity,
            TerminalSize.Normalize(body.Columns ?? 80, body.Rows ?? 24),
            cancellationToken).ConfigureAwait(false);
        return JsonSerializer.SerializeToElement(new
        {
            terminal_session_id = sessionId,
            snapshot = session.Snapshot(500),
            busy = session.IsBusy,
        }, JsonOptions);
    }

    private async Task<RelayHandlerResult> ExecuteCommandAsync(
        RelayRequest request,
        CancellationToken cancellationToken)
    {
        if (_approvals is null || _commandExecutor is null)
        {
            throw new RelayRequestException(503, "Terminal command execution is not configured.");
        }

        var workspace = _workspaces.Find(request.WorkspaceId)
            ?? throw new RelayRequestException(400, "Terminal workspace is not registered locally.");
        var body = Deserialize<TerminalExecBody>(request.Body);
        var networkPolicy = ValidateControlledPolicyScope(body.NetworkPolicy, request);
        var command = Required(body.Command, "command");
        var arguments = body.Arguments ?? [];
        ValidateCommand(command, arguments);
        var paths = new WorkspacePathGuard(workspace.AbsoluteRoot);
        var projectRelative = request.Header("x-local-connector-project-root")
            ?? request.Header("x-local-connector-cwd")
            ?? ".";
        var projectRoot = paths.ResolveExisting(projectRelative);
        if (!Directory.Exists(projectRoot))
        {
            throw new RelayRequestException(400, "Terminal project root is not a directory.");
        }

        var relativeWorkingDirectory = CombineRelative(projectRelative, body.WorkingDirectory ?? ".");
        var workingDirectory = paths.ResolveExisting(relativeWorkingDirectory);
        if (!Directory.Exists(workingDirectory))
        {
            throw new RelayRequestException(400, "Terminal working directory is not a directory.");
        }

        var source = string.IsNullOrWhiteSpace(body.Source)
            ? "terminal-relay"
            : body.Source.Trim();
        var risk = _riskEvaluator.Evaluate(command, arguments);
        var approvalRequest = new CommandApprovalRequest(
            request.RequestId,
            Required(request.OwnerUserId, "owner_user_id"),
            Required(request.DeviceId, "device_id"),
            request.WorkspaceId,
            command,
            arguments,
            workingDirectory,
            source,
            CreateScopeKey(request.WorkspaceId, projectRoot, workingDirectory, command, arguments));
        var approval = await _approvals.RequestAsync(
            approvalRequest,
            risk,
            cancellationToken).ConfigureAwait(false);
        if (!approval.Approved)
        {
            return RelayHandlerResult.Ok(ResponseBody(
                request.WorkspaceId,
                command,
                arguments,
                workingDirectory,
                result: null,
                approval,
                error: approval.Reason,
                auditPersisted: null));
        }

        var timeout = WindowsTerminalCommandExecutor.NormalizeTimeout(body.TimeoutMilliseconds ?? 0);
        var result = await _commandExecutor.ExecuteAsync(new TerminalCommandRequest(
            command,
            arguments,
            workingDirectory,
            paths.Root,
            request.WorkspaceId,
            timeout,
            networkPolicy), cancellationToken).ConfigureAwait(false);
        var auditPersisted = await AppendCommandHistoryAsync(
            request.RequestId,
            source,
            approval,
            result).ConfigureAwait(false);
        return new RelayHandlerResult(
            result.TimedOut ? 408 : 200,
            ResponseBody(
                request.WorkspaceId,
                command,
                arguments,
                workingDirectory,
                result,
                approval,
                result.Error,
                auditPersisted));
    }

    private async Task<bool> AppendCommandHistoryAsync(
        string requestId,
        string source,
        ConnectorApprovalOutcome approval,
        TerminalCommandResult result)
    {
        if (_commandHistory is null)
        {
            return false;
        }

        try
        {
            await _commandHistory.AppendAsync(new TerminalCommandHistoryEntry(
                Guid.NewGuid().ToString("N"),
                requestId,
                result.WorkspaceId,
                source,
                CommandDisplay.Format(result.Command, result.Arguments),
                result.WorkingDirectory,
                result.Success,
                result.ExitCode,
                result.TimedOut,
                result.TimeoutMilliseconds,
                result.StandardOutput,
                result.StandardError,
                result.StandardOutputBytes,
                result.StandardErrorBytes,
                result.StandardOutputTruncated,
                result.StandardErrorTruncated,
                approval.Approved ? "approved" : "denied",
                approval.Reason,
                result.Error,
                _timeProvider.GetUtcNow()), CancellationToken.None).ConfigureAwait(false);
            return true;
        }
        catch
        {
            // Execution already happened and cannot be rolled back. Surface audit status explicitly.
            return false;
        }
    }

    private static JsonElement ResponseBody(
        string workspaceId,
        string command,
        IReadOnlyList<string> arguments,
        string workingDirectory,
        TerminalCommandResult? result,
        ConnectorApprovalOutcome approval,
        string? error,
        bool? auditPersisted) =>
        JsonSerializer.SerializeToElement(new
        {
            command,
            args = arguments,
            cwd = workingDirectory,
            workspace_id = workspaceId,
            success = result?.Success ?? false,
            exit_code = result?.ExitCode,
            timed_out = result?.TimedOut ?? false,
            timeout_ms = result?.TimeoutMilliseconds,
            stdout = result?.StandardOutput ?? string.Empty,
            stderr = result?.StandardError ?? string.Empty,
            stdout_bytes = result?.StandardOutputBytes ?? 0,
            stderr_bytes = result?.StandardErrorBytes ?? 0,
            stdout_truncated = result?.StandardOutputTruncated ?? false,
            stderr_truncated = result?.StandardErrorTruncated ?? false,
            error,
            approval_decision = approval.Approved ? "approved" : "denied",
            approval_reason = approval.Reason,
            approval_mode = Format(approval.Mode),
            approval_reviewer = approval.Reviewer.ToString().ToLowerInvariant(),
            sandbox_profile = result?.SandboxProfile,
            sandbox_network = result?.SandboxNetwork,
            audit_persisted = auditPersisted,
        }, JsonOptions);

    private static string CreateScopeKey(
        string workspaceId,
        string projectRoot,
        string workingDirectory,
        string command,
        IReadOnlyList<string> arguments)
    {
        var identity = string.Join('\0',
            workspaceId,
            Path.GetFullPath(projectRoot),
            Path.GetFullPath(workingDirectory),
            CommandDisplay.Format(command, arguments));
        return Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(identity)))
            .ToLowerInvariant();
    }

    private static string Format(ConnectorApprovalMode mode) => mode switch
    {
        ConnectorApprovalMode.RequestApproval => "request_approval",
        ConnectorApprovalMode.AutoApproval => "auto_approval",
        ConnectorApprovalMode.FullControl => "full_control",
        _ => "request_approval",
    };

    private static void ValidateCommand(string command, IReadOnlyList<string> arguments)
    {
        if (command.Length > 32_768 ||
            arguments.Count > 4_096 ||
            arguments.Sum(value => value?.Length ?? 0) > 128 * 1_024 ||
            arguments.Any(value => value is null || value.IndexOf('\0') >= 0) ||
            command.IndexOf('\0') >= 0)
        {
            throw new RelayRequestException(400, "Terminal command is too large or invalid.");
        }
    }

    private static string CombineRelative(string basePath, string requestedPath)
    {
        var baseComponents = WorkspacePathGuard.NormalizeComponents(basePath, permitsRoot: true);
        var requestedComponents = WorkspacePathGuard.NormalizeComponents(requestedPath, permitsRoot: true);
        if (baseComponents.Length == 0)
        {
            return requestedComponents.Length == 0 ? "." : string.Join('/', requestedComponents);
        }

        if (requestedComponents.Length == 0)
        {
            return string.Join('/', baseComponents);
        }

        var requested = string.Join('/', requestedComponents);
        var baseValue = string.Join('/', baseComponents);
        return requested == baseValue || requested.StartsWith(baseValue + "/", StringComparison.Ordinal)
            ? requested
            : $"{baseValue}/{requested}";
    }

    private static T Deserialize<T>(JsonElement value)
    {
        try
        {
            return value.Deserialize<T>(JsonOptions)
                ?? throw new RelayRequestException(400, "Terminal Relay body is empty.");
        }
        catch (JsonException exception)
        {
            throw new RelayRequestException(400, $"Terminal Relay body is invalid: {exception.Message}");
        }
    }

    private static string Required(string? value, string field) =>
        !string.IsNullOrWhiteSpace(value)
            ? value.Trim()
            : throw new RelayRequestException(400, $"Terminal Relay is missing {field}.");

    private static ControlledNetworkPolicyEnvelope? ValidateControlledPolicyScope(
        ControlledNetworkPolicyEnvelope? policy,
        RelayRequest request)
    {
        if (policy is null) return null;
        if (!string.Equals(policy.OwnerUserId, request.OwnerUserId, StringComparison.Ordinal) ||
            !string.Equals(policy.DeviceId, request.DeviceId, StringComparison.Ordinal) ||
            !string.Equals(policy.WorkspaceId, request.WorkspaceId, StringComparison.Ordinal))
        {
            throw new RelayRequestException(
                400,
                "Controlled network policy identity does not match the Relay request.");
        }
        return policy;
    }

    private sealed record TerminalCreateBody
    {
        [JsonPropertyName("terminal_session_id")]
        public string? TerminalSessionId { get; init; }

        [JsonPropertyName("cwd")]
        public string? WorkingDirectory { get; init; }

        [JsonPropertyName("cols")]
        public int? Columns { get; init; }

        [JsonPropertyName("rows")]
        public int? Rows { get; init; }

        [JsonPropertyName("network_policy")]
        public ControlledNetworkPolicyEnvelope? NetworkPolicy { get; init; }
    }

    private sealed record TerminalControlBody
    {
        [JsonPropertyName("terminal_session_id")]
        public string? TerminalSessionId { get; init; }

        [JsonPropertyName("data")]
        public string? Data { get; init; }

        [JsonPropertyName("command")]
        public string? Command { get; init; }

        [JsonPropertyName("cols")]
        public int? Columns { get; init; }

        [JsonPropertyName("rows")]
        public int? Rows { get; init; }

        [JsonPropertyName("lines")]
        public int? Lines { get; init; }
    }

    private sealed record TerminalExecBody
    {
        [JsonPropertyName("command")]
        public string? Command { get; init; }

        [JsonPropertyName("args")]
        public string[]? Arguments { get; init; }

        [JsonPropertyName("cwd")]
        public string? WorkingDirectory { get; init; }

        [JsonPropertyName("timeout_ms")]
        public int? TimeoutMilliseconds { get; init; }

        [JsonPropertyName("source")]
        public string? Source { get; init; }

        [JsonPropertyName("network_policy")]
        public ControlledNetworkPolicyEnvelope? NetworkPolicy { get; init; }
    }
}
