using System.Text.Json;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.LocalAgent;

internal sealed record WindowsLocalToolInvocation(
    string InvocationId,
    string RunId,
    string BatchId,
    string CallId,
    string ToolName,
    JsonElement Arguments,
    bool SideEffecting,
    bool RequiresApproval,
    string ApprovalStatus,
    string? ApprovalDecidedBy,
    string? ApprovalReason,
    long? ApprovalDecidedAtUnixMs,
    string Status,
    JsonElement? Result,
    string? Error,
    ulong Version,
    string? ClaimToken,
    long? ClaimUntilUnixMs,
    long CreatedAtUnixMs,
    long UpdatedAtUnixMs);
internal sealed record WindowsLocalToolClaim(
    string WorkerId, string ClaimToken, WindowsLocalToolInvocation Invocation);
internal sealed record ClaimLocalToolCommand(
    string Type, string OwnerUserId, string WorkerId, ulong LeaseDurationMs,
    IReadOnlyList<string>? IncludeToolNames, IReadOnlyList<string> ExcludeToolNames);
internal sealed record ClaimLocalToolResult(string Type, WindowsLocalToolClaim? Claim);
internal sealed record CommitLocalToolCommand(
    string Type, string OwnerUserId, string InvocationId, string ClaimToken,
    ulong ExpectedVersion, JsonElement Outcome);
internal sealed record CommitLocalToolResult(string Type, JsonElement Result);
internal sealed record RenewLocalToolClaimCommand(
    string Type, string OwnerUserId, string InvocationId, string ClaimToken,
    ulong ExpectedVersion, ulong LeaseDurationMs);
internal sealed record RenewLocalToolClaimResult(string Type, bool Renewed);
internal sealed record GetLocalRunCommand(string Type, string OwnerUserId, string RunId);
internal sealed record GetLocalRunResult(string Type, WindowsLocalAgentRun Run);

public sealed class WindowsLocalAgentPlatformToolWorker
{
    private const string AttachmentTool = "local_attachment_read";
    private readonly ILocalAgentHostClient _host;
    private readonly WindowsLocalAgentConversationClient _conversations;
    private readonly WindowsLocalAgentAttachmentVault _vault;
    private readonly WindowsLocalAgentProjectToolExecutor? _projectTools;
    private readonly IWindowsLocalAgentPluginToolExecutor? _pluginTools;
    private readonly WindowsLocalAgentExternalMcpExecutor? _externalMcps;
    private readonly WindowsLocalAgentToolApprovalHandler? _approvals;
    private readonly WindowsLocalAgentEventHub? _eventHub;
    private readonly object _gate = new();
    private string? _owner;
    private CancellationTokenSource? _polling;
    private CancellationTokenSource? _eventMonitoring;
    private bool _pendingWake;

    public WindowsLocalAgentPlatformToolWorker(
        ILocalAgentHostClient host,
        WindowsLocalAgentConversationClient conversations,
        WindowsLocalAgentAttachmentVault vault,
        WindowsLocalAgentProjectToolExecutor projectTools,
        IWindowsLocalAgentPluginToolExecutor pluginTools,
        WindowsLocalAgentExternalMcpExecutor externalMcps,
        WindowsLocalAgentToolApprovalHandler approvals,
        WindowsLocalAgentEventHub eventHub)
    {
        _host = host;
        _conversations = conversations;
        _vault = vault;
        _projectTools = projectTools;
        _pluginTools = pluginTools;
        _externalMcps = externalMcps;
        _approvals = approvals;
        _eventHub = eventHub;
    }

    internal WindowsLocalAgentPlatformToolWorker(
        ILocalAgentHostClient host,
        WindowsLocalAgentConversationClient conversations,
        WindowsLocalAgentAttachmentVault vault)
    {
        _host = host;
        _conversations = conversations;
        _vault = vault;
    }

    public void Configure(string ownerUserId)
    {
        Configure(ownerUserId, [], new HashSet<string>(StringComparer.Ordinal));
    }

    public IReadOnlyList<WindowsLocalAgentExternalMcpTool> Configure(
        string ownerUserId,
        IReadOnlyList<ChatOS.Connector.Gateway.ConnectorResolvedMcp> mcps,
        IReadOnlySet<string> selectableExternalMcpIds)
    {
        Reset();
        var externalTools = _externalMcps?.Configure(mcps, selectableExternalMcpIds) ?? [];
        lock (_gate) _owner = ownerUserId;
        _eventHub?.Configure(ownerUserId);
        StartEventMonitoring(ownerUserId);
        Start(TimeSpan.Zero);
        return externalTools;
    }

    public void Reset()
    {
        lock (_gate)
        {
            _owner = null;
            _pendingWake = false;
            _pluginTools?.Reset();
            _externalMcps?.Reset();
            _polling?.Cancel();
            _polling = null;
            _eventMonitoring?.Cancel();
            _eventMonitoring = null;
        }
    }

    public void Wake() => Start(TimeSpan.FromSeconds(15));

    internal Task<IReadOnlyList<WindowsLocalAgentPluginChoice>> ListPluginChoicesAsync(
        CancellationToken cancellationToken) =>
        _pluginTools?.ListChoicesAsync(cancellationToken)
        ?? Task.FromResult<IReadOnlyList<WindowsLocalAgentPluginChoice>>([]);

    private void StartEventMonitoring(string owner)
    {
        if (_eventHub is null) return;
        CancellationTokenSource source;
        lock (_gate)
        {
            if (_eventMonitoring is not null) return;
            source = new CancellationTokenSource();
            _eventMonitoring = source;
        }
        _ = MonitorEventsAsync(owner, source);
    }

    private async Task MonitorEventsAsync(string owner, CancellationTokenSource source)
    {
        try
        {
            await foreach (var update in _eventHub!.UpdatesAsync(source.Token).ConfigureAwait(false))
            {
                if (!string.Equals(update.OwnerUserId, owner, StringComparison.Ordinal) ||
                    update.IsReconcile) continue;
                if (update.Events.Any(value => value.EventType is
                    "tool_batch_requested" or "tool_invocation_approved" or
                    "tool_claim_expired_requeued"))
                {
                    Start(TimeSpan.FromSeconds(15));
                }
            }
        }
        catch (OperationCanceledException)
        {
        }
        finally
        {
            lock (_gate)
            {
                if (ReferenceEquals(_eventMonitoring, source)) _eventMonitoring = null;
            }
            source.Dispose();
        }
    }

    private void Start(TimeSpan window)
    {
        lock (_gate)
        {
            if (_owner is null) return;
            if (_polling is not null)
            {
                if (window != TimeSpan.Zero) _pendingWake = true;
                return;
            }
            _polling = new CancellationTokenSource();
            _ = PollAsync(_owner, window, _polling);
        }
    }

    private async Task PollAsync(string owner, TimeSpan window, CancellationTokenSource source)
    {
        var deadline = DateTimeOffset.UtcNow + window;
        var delay = 250;
        try
        {
            while (!source.IsCancellationRequested)
            {
                if (_approvals is not null &&
                    await _approvals.ResolveNextPendingAsync(owner, source.Token)
                        .ConfigureAwait(false))
                {
                    delay = 250;
                    continue;
                }
                var result = await _host.SendAsync<ClaimLocalToolCommand, ClaimLocalToolResult>(new(
                    "claim_next_tool", owner, "windows-platform-tool-worker", 30_000,
                    WindowsLocalAgentCapabilityCatalog.TaskExecutionToolNames
                        .Concat(
                            _externalMcps?.ToolNames() ??
                            new HashSet<string>(StringComparer.Ordinal))
                        .ToHashSet(StringComparer.Ordinal),
                    ["create_task", "create_tasks_with_prerequisites"]), source.Token)
                    .ConfigureAwait(false);
                if (result.Type != "tool_claim") throw new InvalidDataException("Invalid tool claim result.");
                if (result.Claim is { } claim)
                {
                    var outcome = await ExecuteWhileRenewingAsync(owner, claim, source.Token)
                        .ConfigureAwait(false);
                    if (outcome is null) return;
                    _ = await _host.SendAsync<CommitLocalToolCommand, CommitLocalToolResult>(new(
                        "commit_tool", owner, claim.Invocation.InvocationId, claim.ClaimToken,
                        claim.Invocation.Version, outcome.Value), source.Token).ConfigureAwait(false);
                    delay = 250;
                    continue;
                }
                if (window == TimeSpan.Zero || DateTimeOffset.UtcNow >= deadline) return;
                await Task.Delay(delay, source.Token).ConfigureAwait(false);
                delay = Math.Min(delay * 2, 2_000);
            }
        }
        catch (OperationCanceledException) { }
        catch { if (window == TimeSpan.Zero) return; }
        finally
        {
            var restart = false;
            lock (_gate)
            {
                if (ReferenceEquals(_polling, source))
                {
                    _polling = null;
                    restart = _pendingWake && _owner == owner;
                    _pendingWake = false;
                }
            }
            source.Dispose();
            if (restart) Start(TimeSpan.FromSeconds(15));
        }
    }

    private async Task<JsonElement?> ExecuteWhileRenewingAsync(
        string owner,
        WindowsLocalToolClaim claim,
        CancellationToken cancellationToken)
    {
        using var execution = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        var executeTask = ExecuteAsync(owner, claim, execution.Token);
        var heartbeatTask = RenewUntilCancelledAsync(owner, claim, execution.Token);
        var completed = await Task.WhenAny(executeTask, heartbeatTask).ConfigureAwait(false);
        if (completed == executeTask)
        {
            var outcome = await executeTask.ConfigureAwait(false);
            execution.Cancel();
            try
            {
                if (!await heartbeatTask.ConfigureAwait(false)) return null;
            }
            catch (OperationCanceledException) { }
            return outcome;
        }

        var leaseRetained = await heartbeatTask.ConfigureAwait(false);
        if (leaseRetained) return await executeTask.ConfigureAwait(false);
        execution.Cancel();
        try { await executeTask.ConfigureAwait(false); }
        catch (OperationCanceledException) { }
        return null;
    }

    private async Task<bool> RenewUntilCancelledAsync(
        string owner,
        WindowsLocalToolClaim claim,
        CancellationToken cancellationToken)
    {
        try
        {
            while (true)
            {
                await Task.Delay(TimeSpan.FromSeconds(10), cancellationToken).ConfigureAwait(false);
                var result = await _host.SendAsync<RenewLocalToolClaimCommand, RenewLocalToolClaimResult>(
                    new("renew_tool_claim", owner, claim.Invocation.InvocationId, claim.ClaimToken,
                        claim.Invocation.Version, 30_000), cancellationToken).ConfigureAwait(false);
                if (result.Type != "tool_claim_renewed") return false;
                if (!result.Renewed) return false;
            }
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch
        {
            return false;
        }
    }

    private async Task<JsonElement> ExecuteAsync(
        string owner, WindowsLocalToolClaim claim, CancellationToken cancellationToken)
    {
        try
        {
            if (_externalMcps?.ToolNames().Contains(claim.Invocation.ToolName) == true)
            {
                var externalOutput = await _externalMcps.ExecuteAsync(
                    owner,
                    claim.Invocation,
                    cancellationToken).ConfigureAwait(false);
                return JsonSerializer.SerializeToElement(new { type = "succeeded", output = externalOutput });
            }
            if (WindowsLocalAgentCapabilityCatalog.PluginToolNames.Contains(
                    claim.Invocation.ToolName))
            {
                if (_pluginTools is null)
                    throw new InvalidOperationException("Local Plugin tools are unavailable.");
                var pluginOutput = await _pluginTools.ExecuteAsync(
                    owner,
                    claim.Invocation.RunId,
                    claim.Invocation.CallId,
                    claim.Invocation.ToolName,
                    claim.Invocation.Arguments,
                    cancellationToken).ConfigureAwait(false);
                return JsonSerializer.SerializeToElement(new { type = "succeeded", output = pluginOutput });
            }
            if (WindowsLocalAgentCapabilityCatalog.ProjectToolNames.Contains(
                    claim.Invocation.ToolName))
            {
                if (_projectTools is null)
                    throw new InvalidOperationException("Local project tools are unavailable.");
                var projectOutput = await _projectTools.ExecuteAsync(
                    owner, claim.Invocation, cancellationToken).ConfigureAwait(false);
                return JsonSerializer.SerializeToElement(new { type = "succeeded", output = projectOutput });
            }
            if (claim.Invocation.ToolName != AttachmentTool ||
                claim.Invocation.Arguments.ValueKind != JsonValueKind.Object)
                throw new InvalidOperationException("Unsupported local platform tool.");
            var args = claim.Invocation.Arguments;
            var reference = args.GetProperty("authorized_local_ref").GetString()
                ?? throw new InvalidOperationException("Invalid attachment reference.");
            var offset = args.TryGetProperty("offset", out var offsetValue) ? offsetValue.GetUInt64() : 0;
            var limit = args.TryGetProperty("limit", out var limitValue) ? limitValue.GetInt32() : 16_384;
            var runResult = await _host.SendAsync<GetLocalRunCommand, GetLocalRunResult>(
                new("get_run", owner, claim.Invocation.RunId), cancellationToken).ConfigureAwait(false);
            var hasConversation = runResult.Run.Input.TryGetProperty(
                "source_conversation_id", out var conversationValue) ||
                runResult.Run.Input.TryGetProperty("conversation_id", out conversationValue);
            if (runResult.Type != "run" || runResult.Run.OwnerUserId != owner ||
                !hasConversation)
                throw new InvalidOperationException("Invalid Local Agent Run context.");
            var conversationId = conversationValue.GetString()
                ?? throw new InvalidOperationException("Invalid Local Agent conversation context.");
            var conversation = await _conversations.GetAsync(owner, conversationId, cancellationToken)
                .ConfigureAwait(false);
            var attachment = conversation.Attachments.FirstOrDefault(value =>
                value.AuthorizedLocalRef == reference) ?? throw new InvalidOperationException(
                    "The attachment is not authorized for this conversation.");
            var attachmentOutput = _vault.Resolve(attachment, owner, conversationId, offset, limit);
            return JsonSerializer.SerializeToElement(new { type = "succeeded", output = attachmentOutput });
        }
        catch
        {
            return JsonSerializer.SerializeToElement(new {
                type = claim.Invocation.SideEffecting ? "needs_review" : "failed",
                error = "Local platform tool failed.",
                reason = "Local platform tool failed.",
                detail = new { tool_name = claim.Invocation.ToolName, phase = "native_platform_execution" },
            });
        }
    }
}
