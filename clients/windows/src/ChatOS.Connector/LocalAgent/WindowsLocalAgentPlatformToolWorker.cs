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
internal sealed record GetLocalRunCommand(string Type, string OwnerUserId, string RunId);
internal sealed record GetLocalRunResult(string Type, WindowsLocalAgentRun Run);

public sealed class WindowsLocalAgentPlatformToolWorker
{
    private const string AttachmentTool = "local_attachment_read";
    private readonly ILocalAgentHostClient _host;
    private readonly WindowsLocalAgentConversationClient _conversations;
    private readonly WindowsLocalAgentAttachmentVault _vault;
    private readonly WindowsLocalAgentProjectToolExecutor? _projectTools;
    private readonly WindowsLocalAgentToolApprovalHandler? _approvals;
    private readonly object _gate = new();
    private string? _owner;
    private CancellationTokenSource? _polling;
    private bool _pendingWake;

    public WindowsLocalAgentPlatformToolWorker(
        ILocalAgentHostClient host,
        WindowsLocalAgentConversationClient conversations,
        WindowsLocalAgentAttachmentVault vault,
        WindowsLocalAgentProjectToolExecutor projectTools,
        WindowsLocalAgentToolApprovalHandler approvals)
    {
        _host = host;
        _conversations = conversations;
        _vault = vault;
        _projectTools = projectTools;
        _approvals = approvals;
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
        Reset();
        lock (_gate) _owner = ownerUserId;
        Start(TimeSpan.Zero);
    }

    public void Reset()
    {
        lock (_gate)
        {
            _owner = null;
            _pendingWake = false;
            _polling?.Cancel();
            _polling = null;
        }
    }

    public void Wake() => Start(TimeSpan.FromMinutes(5));

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
                    [AttachmentTool, .. WindowsLocalAgentCapabilityCatalog.TaskExecutionToolNames],
                    ["create_task", "create_tasks_with_prerequisites"]), source.Token)
                    .ConfigureAwait(false);
                if (result.Type != "tool_claim") throw new InvalidDataException("Invalid tool claim result.");
                if (result.Claim is { } claim)
                {
                    var outcome = await ExecuteAsync(owner, claim, source.Token).ConfigureAwait(false);
                    _ = await _host.SendAsync<CommitLocalToolCommand, CommitLocalToolResult>(new(
                        "commit_tool", owner, claim.Invocation.InvocationId, claim.ClaimToken,
                        claim.Invocation.Version, outcome), source.Token).ConfigureAwait(false);
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
            if (restart) Start(TimeSpan.FromMinutes(5));
        }
    }

    private async Task<JsonElement> ExecuteAsync(
        string owner, WindowsLocalToolClaim claim, CancellationToken cancellationToken)
    {
        try
        {
            if (WindowsLocalAgentCapabilityCatalog.TaskExecutionToolNames.Contains(
                    claim.Invocation.ToolName))
            {
                if (_projectTools is null)
                    throw new InvalidOperationException("Local project tools are unavailable.");
                var output = await _projectTools.ExecuteAsync(
                    owner, claim.Invocation, cancellationToken).ConfigureAwait(false);
                return JsonSerializer.SerializeToElement(new { type = "succeeded", output });
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
            if (runResult.Type != "run" || runResult.Run.OwnerUserId != owner ||
                !runResult.Run.Input.TryGetProperty("conversation_id", out var conversationValue))
                throw new InvalidOperationException("Invalid Local Agent Run context.");
            var conversationId = conversationValue.GetString()
                ?? throw new InvalidOperationException("Invalid Local Agent conversation context.");
            var conversation = await _conversations.GetAsync(owner, conversationId, cancellationToken)
                .ConfigureAwait(false);
            var attachment = conversation.Attachments.FirstOrDefault(value =>
                value.AuthorizedLocalRef == reference) ?? throw new InvalidOperationException(
                    "The attachment is not authorized for this conversation.");
            var output = _vault.Resolve(attachment, owner, conversationId, offset, limit);
            return JsonSerializer.SerializeToElement(new { type = "succeeded", output });
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
