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

public sealed class WindowsLocalAgentRuntimeClient(ILocalAgentHostClient host)
{
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
}
