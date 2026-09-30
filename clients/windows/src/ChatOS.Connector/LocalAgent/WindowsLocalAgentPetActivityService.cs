using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentPetActivityService : IPetActivityInboxService
{
    private readonly WindowsLocalAgentRuntimeClient _runtime;
    private readonly object _gate = new();
    private string? _ownerUserId;

    public WindowsLocalAgentPetActivityService(WindowsLocalAgentRuntimeClient runtime)
    {
        _runtime = runtime;
    }

    public void Configure(string ownerUserId)
    {
        lock (_gate) _ownerUserId = ownerUserId;
    }

    public void Reset()
    {
        lock (_gate) _ownerUserId = null;
    }

    public async Task<IReadOnlyList<PetActivity>> FetchOpenActivitiesAsync(
        int limit = 100,
        CancellationToken cancellationToken = default)
    {
        var owner = RequireOwner();
        var normalized = (uint)Math.Clamp(limit, 1, 100);
        var activeTask = _runtime.ListRunsAsync(owner, "active", normalized, cancellationToken);
        var terminalTask = _runtime.ListRunsAsync(owner, "terminal", normalized, cancellationToken);
        await Task.WhenAll(activeTask, terminalTask).ConfigureAwait(false);
        var cutoff = DateTimeOffset.UtcNow.AddMinutes(-15);
        return activeTask.Result.Runs.Concat(terminalTask.Result.Runs)
            .Where(run => !IsTerminal(run.Status) || Date(run.UpdatedAtUnixMs) >= cutoff)
            .OrderByDescending(run => run.UpdatedAtUnixMs)
            .Take((int)normalized)
            .Select(Map)
            .ToArray();
    }

    public Task ApplyAsync(
        PetActivityDisposition disposition,
        PetActivity activity,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        return Task.CompletedTask;
    }

    private static PetActivity Map(WindowsLocalAgentRun run)
    {
        var source = run.ProfileKey == "main_chat"
            ? PetActivitySource.Chat
            : PetActivitySource.TaskExecution;
        var conversationId = String(run.Input, "conversation_id")
            ?? String(run.Input, "source_conversation_id");
        var turnId = String(run.Input, "turn_id")
            ?? String(run.Input, "source_turn_id")
            ?? (run.OwnerEntityType == "conversation_turn" ? run.OwnerEntityId : null);
        var taskId = run.OwnerEntityType == "task" ? run.OwnerEntityId : null;
        var kind = Kind(run.Status);
        var updated = Date(run.UpdatedAtUnixMs);
        return new PetActivity(
            $"local-run:{run.RunId}",
            source,
            kind,
            source == PetActivitySource.Chat ? "本地对话" : "本地任务",
            Detail(run),
            new PetActivityRoute(
                ConversationId: conversationId,
                TurnId: turnId,
                PromptId: run.Status == "waiting_user" ? $"local-ask:{run.RunId}" : null,
                TaskId: taskId,
                RunId: run.RunId),
            $"local-run:{run.RunId}:{run.Version}",
            Clamp(run.Version),
            activityVersion: run.Version.ToString(),
            updatedAt: updated,
            expiresAt: kind is PetActivityKind.Succeeded or PetActivityKind.Cancelled
                ? updated.AddSeconds(15)
                : null);
    }

    private static PetActivityKind Kind(string status) => status switch
    {
        "waiting_user" => PetActivityKind.WaitingForUser,
        "paused" or "needs_review" => PetActivityKind.Reviewing,
        "succeeded" => PetActivityKind.Succeeded,
        "failed" => PetActivityKind.Failed,
        "cancelled" => PetActivityKind.Cancelled,
        _ => PetActivityKind.Working,
    };

    private static string? Detail(WindowsLocalAgentRun run)
    {
        if (run.TerminalOutcome is { } outcome)
            return String(outcome, "error") ?? String(outcome, "text") ?? String(outcome, "reason");
        return run.Status switch
        {
            "waiting_user" => "等待你的回复",
            "waiting_tool_result" => "正在执行本地工具",
            "retry_scheduled" => "等待本地重试",
            "needs_review" => "需要检查执行结果",
            _ => null,
        };
    }

    private static string? String(JsonElement value, string property) =>
        value.ValueKind == JsonValueKind.Object &&
        value.TryGetProperty(property, out var result) &&
        result.ValueKind == JsonValueKind.String
            ? result.GetString()
            : null;

    private string RequireOwner()
    {
        lock (_gate)
        {
            return _ownerUserId ?? throw new InvalidOperationException(
                "Local Agent pet activities are not configured.");
        }
    }

    private static bool IsTerminal(string status) =>
        status is "succeeded" or "failed" or "cancelled";
    private static DateTimeOffset Date(long unixMilliseconds) =>
        DateTimeOffset.FromUnixTimeMilliseconds(unixMilliseconds);
    private static long Clamp(ulong value) => value > long.MaxValue ? long.MaxValue : (long)value;
}
