using System.Text.Json;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.LocalAgent;

internal sealed record WindowsLocalAgentRun(
    string RunId,
    string OwnerUserId,
    string OwnerEntityType,
    string OwnerEntityId,
    string ProfileKey,
    JsonElement Input,
    string Status,
    ulong Version,
    JsonElement? TerminalOutcome,
    long CreatedAtUnixMs,
    long UpdatedAtUnixMs);

internal sealed record WindowsLocalAgentRunPage(
    IReadOnlyList<WindowsLocalAgentRun> Runs,
    long? NextBeforeUpdatedAtUnixMs,
    string? NextBeforeRunId);

internal sealed record ListLocalRunsCommand(
    string Type,
    string OwnerUserId,
    string Scope,
    long? BeforeUpdatedAtUnixMs,
    string? BeforeRunId,
    uint Limit);

internal sealed record ListLocalRunsResult(string Type, WindowsLocalAgentRunPage Page);
internal sealed record WindowsLocalAgentEvent(
    long Cursor, string EventId, string RunId, string EventType,
    JsonElement Payload, long CreatedAtUnixMs);
internal sealed record ListLocalEventsCommand(
    string Type, string OwnerUserId, long AfterCursor, uint Limit, string? RunId);
internal sealed record ListLocalEventsResult(
    string Type, IReadOnlyList<WindowsLocalAgentEvent> Events, long NextCursor);
internal sealed record WindowsLocalAgentEventPage(
    IReadOnlyList<WindowsLocalAgentEvent> Events, long NextCursor);
internal sealed record ResumeLocalRunCommand(
    string Type, string OwnerUserId, string RunId, ulong ExpectedVersion,
    string ExpectedStatus, string Reason, JsonElement Input);
internal sealed record CancelLocalTaskCommand(
    string Type, string OwnerUserId, string TaskId, ulong? ExpectedVersion, string Reason);
internal sealed record LocalTaskGraphResult(string Type, JsonElement Graph);

public sealed class WindowsLocalAgentRuntimeClient(ILocalAgentHostClient host)
{
    internal async Task<WindowsLocalAgentRun> GetRunAsync(
        string ownerUserId, string runId, CancellationToken cancellationToken = default)
    {
        var result = await host.SendAsync<GetLocalRunCommand, GetLocalRunResult>(
            new("get_run", ownerUserId, runId), cancellationToken).ConfigureAwait(false);
        return result.Type == "run" ? result.Run : throw new InvalidDataException("Invalid Run result.");
    }

    internal async Task<WindowsLocalAgentRunPage> ListRunsAsync(
        string ownerUserId,
        string scope,
        uint limit,
        CancellationToken cancellationToken = default)
    {
        var result = await host.SendAsync<ListLocalRunsCommand, ListLocalRunsResult>(new(
            "list_runs",
            ownerUserId,
            scope,
            null,
            null,
            limit), cancellationToken).ConfigureAwait(false);
        return result.Type == "runs"
            ? result.Page
            : throw new InvalidDataException("Local Agent Host returned an invalid Runs result.");
    }

    internal async Task<IReadOnlyList<WindowsLocalAgentEvent>> ListEventsAsync(
        string ownerUserId, string runId, CancellationToken cancellationToken = default)
    {
        var page = await ListEventPageAsync(
            ownerUserId, 0, runId, 500, cancellationToken).ConfigureAwait(false);
        return page.Events;
    }

    internal async Task<WindowsLocalAgentEventPage> ListEventPageAsync(
        string ownerUserId,
        long afterCursor,
        string? runId = null,
        uint limit = 100,
        CancellationToken cancellationToken = default)
    {
        var result = await host.SendAsync<ListLocalEventsCommand, ListLocalEventsResult>(new(
            "list_events", ownerUserId, afterCursor, limit, runId), cancellationToken)
            .ConfigureAwait(false);
        return result.Type == "events"
            ? new WindowsLocalAgentEventPage(result.Events, result.NextCursor)
            : throw new InvalidDataException("Invalid Events result.");
    }

    internal async Task<WindowsLocalAgentRun> ResumeWaitingRunAsync(
        string ownerUserId, WindowsLocalAgentRun run, JsonElement input,
        CancellationToken cancellationToken = default)
    {
        var result = await host.SendAsync<ResumeLocalRunCommand, GetLocalRunResult>(new(
            "resume_run", ownerUserId, run.RunId, run.Version, "waiting_user",
            "ask_user_submitted", input), cancellationToken).ConfigureAwait(false);
        return result.Type == "run" ? result.Run : throw new InvalidDataException("Invalid Run result.");
    }

    internal async Task CancelTaskAsync(
        string ownerUserId, string taskId, CancellationToken cancellationToken = default)
    {
        var result = await host.SendAsync<CancelLocalTaskCommand, LocalTaskGraphResult>(new(
            "cancel_task", ownerUserId, taskId, null, "user_cancelled"), cancellationToken)
            .ConfigureAwait(false);
        if (result.Type != "task_graph") throw new InvalidDataException("Invalid Task Graph result.");
    }
}
