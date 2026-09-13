using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

/// <summary>
/// Projects the account-scoped Local Agent authority into Pet activities.
/// There is no server inbox, realtime socket, disposition API, or alternate
/// cancellation path behind this service.
/// </summary>
public sealed class WindowsLocalAgentPetActivityService : ILocalAgentPetActivityService, IDisposable
{
    private static readonly TimeSpan TerminalVisibility = TimeSpan.FromDays(30);
    private readonly IWindowsLocalAgentProjectionStore _projectionStore;
    private readonly IPetActivitySuppressionStore _suppressionStore;
    private readonly SemaphoreSlim _gate = new(1, 1);
    private bool _disposed;

    public WindowsLocalAgentPetActivityService(
        IWindowsLocalAgentProjectionStore projectionStore,
        IPetActivitySuppressionStore suppressionStore)
    {
        _projectionStore = projectionStore;
        _suppressionStore = suppressionStore;
        _projectionStore.Changed += OnProjectionChanged;
        _projectionStore.Cleared += OnProjectionCleared;
    }

    public event EventHandler? Changed;

    public async Task<IReadOnlyList<PetActivity>> FetchAsync(
        CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            var now = DateTimeOffset.UtcNow;
            await _suppressionStore.PruneExpiredAsync(now, cancellationToken).ConfigureAwait(false);
            var projection = await _projectionStore.GetAsync(cancellationToken).ConfigureAwait(false);
            if (projection is null) return [];
            var activities = Project(projection)
                .Where(activity => activity.ExpiresAt is null || activity.ExpiresAt > now)
                .ToArray();
            if (activities.Select(activity => activity.Id).Distinct(StringComparer.Ordinal).Count()
                != activities.Length)
                throw new InvalidDataException(
                    "The Local Agent Pet projection contains duplicate activity identities.");
            var visible = new List<PetActivity>(activities.Length);
            foreach (var activity in activities)
            {
                if (!await _suppressionStore.IsSuppressedAsync(
                        activity.StableIdentity, now, cancellationToken).ConfigureAwait(false))
                    visible.Add(activity);
            }
            return visible.OrderByDescending(activity => activity.PresentationPriority)
                .ThenByDescending(activity => activity.UpdatedAt)
                .ThenBy(activity => activity.Id, StringComparer.Ordinal)
                .ToArray();
        }
        finally
        {
            _gate.Release();
        }
    }

    public async Task SuppressAsync(
        PetActivity activity,
        PetActivityDisposition disposition,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(activity);
        if (disposition is not (PetActivityDisposition.Ignored
            or PetActivityDisposition.Handled))
            throw new ArgumentOutOfRangeException(nameof(disposition));
        ThrowIfDisposed();
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            var projection = await _projectionStore.GetAsync(cancellationToken).ConfigureAwait(false)
                ?? throw new InvalidOperationException(
                    "The Local Agent account projection is not available.");
            var current = Project(projection).Where(candidate =>
                    candidate.Id == activity.Id
                    && candidate.StableIdentity == activity.StableIdentity)
                .ToArray();
            if (current.Length != 1)
                throw new InvalidOperationException(
                    "The Local Agent Pet activity is no longer current.");
            var now = DateTimeOffset.UtcNow;
            var expiry = activity.ExpiresAt is { } activityExpiry && activityExpiry > now
                ? activityExpiry
                : now.Add(TerminalVisibility);
            await _suppressionStore.SuppressAsync(
                activity.StableIdentity, disposition, now, expiry, cancellationToken)
                .ConfigureAwait(false);
        }
        finally
        {
            _gate.Release();
        }
        Changed?.Invoke(this, EventArgs.Empty);
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        _projectionStore.Changed -= OnProjectionChanged;
        _projectionStore.Cleared -= OnProjectionCleared;
        _gate.Dispose();
    }

    internal static IReadOnlyList<PetActivity> Project(
        WindowsLocalAgentProjectionSnapshot projection)
    {
        var activities = new List<PetActivity>();
        foreach (var route in WindowsLocalAgentInteractionProjection.Routes(projection))
        {
            if (WindowsLocalAgentInteractionProjection.TryAskUserPrompt(route, out var prompt))
            {
                activities.Add(new PetActivity(
                    $"local-ask:{prompt.Id}",
                    PetActivitySource.AskUserPrompt,
                    PetActivityKind.WaitingForUser,
                    prompt.Title,
                    prompt.Message,
                    Route(route, promptId: prompt.Id),
                    activityVersion: Version(route.Run),
                    updatedAt: route.Run.UpdatedAt));
                continue;
            }

            var approvals = WindowsLocalAgentInteractionProjection.ToolApprovals(route);
            if (approvals.Count > 0)
            {
                foreach (var approval in approvals)
                {
                    activities.Add(new PetActivity(
                        $"local-tool:{approval.InvocationId}",
                        PetActivitySource.LocalAgentToolApproval,
                        PetActivityKind.WaitingForApproval,
                        approval.ToolName,
                        $"Effect: {approval.Effect} · {ShortDigest(approval.ArgumentsDigest)}",
                        Route(route, invocationId: approval.InvocationId),
                        activityVersion: Version(route.Run),
                        updatedAt: route.Run.UpdatedAt));
                }
                continue;
            }

            activities.Add(RunActivity(route));
        }
        return activities;
    }

    private static PetActivity RunActivity(WindowsLocalAgentProjectionRoute route)
    {
        var run = route.Run;
        var source = route.Source.Task is null
            ? PetActivitySource.Chat
            : PetActivitySource.TaskRunner;
        var kind = run.Status switch
        {
            LocalAgentRunStatus.NeedsReview or LocalAgentRunStatus.Paused =>
                PetActivityKind.NeedsReview,
            LocalAgentRunStatus.Succeeded => PetActivityKind.Succeeded,
            LocalAgentRunStatus.Failed => PetActivityKind.Failed,
            LocalAgentRunStatus.Cancelled => PetActivityKind.Cancelled,
            _ => PetActivityKind.Working,
        };
        var title = route.Source.Task?.Objective ?? (kind switch
        {
            PetActivityKind.Succeeded => "对话回复已完成",
            PetActivityKind.Failed => "对话运行失败",
            PetActivityKind.Cancelled => "对话运行已取消",
            PetActivityKind.NeedsReview => "对话需要人工复核",
            _ => "AI 正在处理对话",
        });
        var control = WindowsLocalAgentInteractionProjection.RunControl(route);
        var detail = control.ReviewReason ?? TerminalDetail(run.TerminalOutcome) ?? kind switch
        {
            PetActivityKind.Working => $"第 {run.Iteration} 轮 · 重试 {run.RetryCount} 次",
            PetActivityKind.Succeeded => "本地 Agent 已完成。",
            PetActivityKind.Failed => "本地 Agent 未能完成。",
            PetActivityKind.Cancelled => "本地 Agent 已取消。",
            _ => "请检查运行过程后决定是否继续。",
        };
        var terminal = kind is PetActivityKind.Succeeded
            or PetActivityKind.Failed or PetActivityKind.Cancelled;
        return new PetActivity(
            route.Source.Task is null
                ? $"local-run:{run.RunId}"
                : $"local-task:{route.Source.Task.TaskId}:{run.RunId}",
            source,
            kind,
            title,
            detail,
            Route(route),
            activityVersion: Version(run),
            updatedAt: run.UpdatedAt,
            expiresAt: terminal ? run.UpdatedAt.Add(TerminalVisibility) : null);
    }

    private static PetActivityRoute Route(
        WindowsLocalAgentProjectionRoute route,
        string? promptId = null,
        string? invocationId = null) => new(
        ProjectId: route.Run.ProjectId,
        ConversationId: route.Source.ThreadId,
        TurnId: route.Source.TurnId,
        MessageId: route.Recovered.MainChatBinding?.MessageId,
        PromptId: promptId,
        TaskId: route.Source.Task?.TaskId,
        RunId: route.Run.RunId,
        InvocationId: invocationId);

    private static string Version(LocalAgentRunSnapshot run) => $"run-version:{run.Version}";

    private static string? TerminalDetail(JsonElement? outcome)
    {
        if (outcome is not { ValueKind: JsonValueKind.Object } value) return null;
        foreach (var property in new[]
                 {
                     "result_summary", "report_content", "error_message", "message", "reason",
                 })
        {
            if (value.TryGetProperty(property, out var item)
                && item.ValueKind == JsonValueKind.String
                && item.GetString()?.Trim() is { Length: > 0 } text)
                return text.Length <= 500 ? text : $"{text[..497]}…";
        }
        return null;
    }

    private static string ShortDigest(string digest) => digest.Length <= 22
        ? digest
        : $"{digest[..15]}…{digest[^6..]}";

    private void OnProjectionChanged(
        object? sender,
        WindowsLocalAgentProjectionSnapshot snapshot) => Changed?.Invoke(this, EventArgs.Empty);

    private void OnProjectionCleared(object? sender, EventArgs args) =>
        Changed?.Invoke(this, EventArgs.Empty);

    private void ThrowIfDisposed() => ObjectDisposedException.ThrowIf(_disposed, this);
}
