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
    string? Status,
    long? UpdatedAfterUnixMs,
    long? BeforeUpdatedAtUnixMs,
    string? BeforeRunId,
    uint Limit);

internal sealed record ListLocalRunsResult(string Type, WindowsLocalAgentRunPage Page);
internal sealed record WindowsLocalAgentEvent(
    long Cursor, string EventId, string RunId, string EventType,
    JsonElement? Payload, long CreatedAtUnixMs);
internal sealed record ListLocalEventsCommand(
    string Type, string OwnerUserId, long AfterCursor, uint Limit, string? RunId,
    string? EventType, bool NewestFirst, string PayloadMode);
internal sealed record WaitLocalEventsCommand(
    string Type, string OwnerUserId, long AfterCursor, uint Limit, string? RunId,
    ulong TimeoutMs, string PayloadMode);
internal sealed record GetLocalEventCursorCommand(string Type, string OwnerUserId);
internal sealed record GetLocalEventCursorResult(string Type, long Cursor);
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
        string? status = null,
        long? updatedAfterUnixMs = null,
        CancellationToken cancellationToken = default)
    {
        var result = await host.SendAsync<ListLocalRunsCommand, ListLocalRunsResult>(new(
            "list_runs",
            ownerUserId,
            scope,
            status,
            updatedAfterUnixMs,
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
            ownerUserId, 0, runId, 500, cancellationToken: cancellationToken)
            .ConfigureAwait(false);
        return page.Events;
    }

    internal async Task<WindowsLocalAgentEventPage> ListEventPageAsync(
        string ownerUserId,
        long afterCursor,
        string? runId = null,
        uint limit = 100,
        string? eventType = null,
        bool newestFirst = false,
        string payloadMode = "full",
        CancellationToken cancellationToken = default)
    {
        var result = await host.SendAsync<ListLocalEventsCommand, ListLocalEventsResult>(new(
            "list_events", ownerUserId, afterCursor, limit, runId, eventType, newestFirst,
            payloadMode), cancellationToken)
            .ConfigureAwait(false);
        return result.Type == "events"
            ? new WindowsLocalAgentEventPage(result.Events, result.NextCursor)
            : throw new InvalidDataException("Invalid Events result.");
    }

    internal async Task<WindowsLocalAgentEventPage> WaitEventsAsync(
        string ownerUserId,
        long afterCursor,
        uint limit = 100,
        ulong timeoutMs = 20_000,
        string payloadMode = "routing",
        CancellationToken cancellationToken = default)
    {
        var result = await host.SendAsync<WaitLocalEventsCommand, ListLocalEventsResult>(new(
            "wait_events", ownerUserId, afterCursor, limit, null, timeoutMs, payloadMode),
            cancellationToken).ConfigureAwait(false);
        return result.Type == "events"
            ? new WindowsLocalAgentEventPage(result.Events, result.NextCursor)
            : throw new InvalidDataException("Invalid Events result.");
    }

    internal async Task<long> GetLatestEventCursorAsync(
        string ownerUserId,
        CancellationToken cancellationToken = default)
    {
        var result = await host.SendAsync<GetLocalEventCursorCommand, GetLocalEventCursorResult>(
            new("get_event_cursor", ownerUserId), cancellationToken).ConfigureAwait(false);
        return result is { Type: "event_cursor", Cursor: >= 0 }
            ? result.Cursor
            : throw new InvalidDataException("Invalid Event Cursor result.");
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
