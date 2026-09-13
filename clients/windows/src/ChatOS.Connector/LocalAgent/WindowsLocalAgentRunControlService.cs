using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentRunControlService(
    IWindowsLocalAgentProjectionStore store,
    IWindowsLocalAgentAccountSession accountSession,
    WindowsLocalAgentRunProjectionRefresher refresher) : ILocalAgentRunControlService
{
    public async Task<IReadOnlyList<LocalAgentRunControlState>> FetchRunControlsAsync(
        string conversationId, CancellationToken cancellationToken = default)
    {
        RequireIdentity(conversationId, nameof(conversationId));
        var projection = await RequireProjectionAsync(cancellationToken).ConfigureAwait(false);
        var routes = Routes(projection, conversationId).Where(route => !route.State.IsTerminal)
            .OrderBy(route => route.State.UpdatedAt).ThenBy(route => route.State.RunId,
                StringComparer.Ordinal).ToArray();
        if (routes.Select(route => route.State.RunId).Distinct(StringComparer.Ordinal).Count()
            != routes.Length)
            throw new InvalidDataException("The Local Agent projection contains duplicate Run identities.");
        return routes.Select(route => route.State).ToArray();
    }

    public Task PauseRunAsync(string runId, string conversationId,
        CancellationToken cancellationToken = default) => MutateAsync(
        runId, conversationId, static state => state.CanPause,
        static state => LocalAgentCommand.PauseRun(state.RunId, state.RunVersion),
        cancellationToken);

    public Task ResumeRunAsync(string runId, string conversationId,
        CancellationToken cancellationToken = default) => MutateAsync(
        runId, conversationId, static state => state.CanResume,
        static state => LocalAgentCommand.ResumeRun(state.RunId, state.RunVersion),
        cancellationToken);

    public Task CancelRunAsync(string runId, string conversationId,
        CancellationToken cancellationToken = default) => MutateAsync(
        runId, conversationId, static state => state.CanCancel,
        static state => LocalAgentCommand.CancelRun(state.RunId, state.RunVersion),
        cancellationToken);

    private async Task MutateAsync(
        string runId, string conversationId, Func<LocalAgentRunControlState, bool> allowed,
        Func<LocalAgentRunControlState, LocalAgentCommand> command,
        CancellationToken cancellationToken)
    {
        RequireIdentity(runId, nameof(runId));
        RequireIdentity(conversationId, nameof(conversationId));
        var projection = await RequireProjectionAsync(cancellationToken).ConfigureAwait(false);
        var matches = Routes(projection, conversationId)
            .Where(route => route.State.RunId == runId).ToArray();
        if (matches.Length != 1 || !allowed(matches[0].State))
            throw new InvalidOperationException("The Local Agent Run does not allow this action.");
        var client = await accountSession.GetClientAsync(projection.AccountId, cancellationToken)
            .ConfigureAwait(false);
        _ = await client.AcceptAsync(command(matches[0].State), cancellationToken)
            .ConfigureAwait(false);
        _ = await refresher.RefreshAsync(projection.AccountId, runId, cancellationToken)
            .ConfigureAwait(false);
    }

    private async Task<WindowsLocalAgentProjectionSnapshot> RequireProjectionAsync(
        CancellationToken cancellationToken) =>
        await store.GetAsync(cancellationToken).ConfigureAwait(false)
        ?? throw new InvalidOperationException("The Local Agent account projection is not available.");

    private static IEnumerable<RunRoute> Routes(
        WindowsLocalAgentProjectionSnapshot projection, string conversationId)
    {
        foreach (var recovered in projection.Runs.Values)
        {
            var source = WindowsLocalAgentRunSourceResolver.Resolve(projection, recovered);
            if (source is null || source.ThreadId != conversationId) continue;
            var run = recovered.Run;
            yield return new RunRoute(run, new LocalAgentRunControlState(
                run.RunId, run.Version, source.ThreadId, source.TurnId, run.Status,
                run.Iteration, run.RetryCount, InteractionKind(run.PendingInteraction),
                ReviewReason(run.PendingInteraction), run.UpdatedAt));
        }
    }

    private static string? InteractionKind(JsonElement? interaction) =>
        interaction is { ValueKind: JsonValueKind.Object } value
        && value.TryGetProperty("type", out var type)
        && type.ValueKind == JsonValueKind.String ? type.GetString() : null;

    private static string? ReviewReason(JsonElement? interaction)
    {
        if (interaction is not { ValueKind: JsonValueKind.Object } value) return null;
        return InteractionKind(interaction) switch
        {
            "review_unknown_tool_outcome" => value.TryGetProperty("batch_id", out var batch)
                && batch.ValueKind == JsonValueKind.String
                ? $"工具批次 {batch.GetString()} 的执行结果无法确认。继续前请核对外部结果，避免重复执行。"
                : "工具执行结果无法确认。继续前请核对外部结果，避免重复执行。",
            "runtime_blocked" => "本地 Agent 已阻塞，需要检查执行过程后再继续。",
            _ => null,
        };
    }

    private static void RequireIdentity(string value, string name)
    {
        if (string.IsNullOrWhiteSpace(value) || value != value.Trim() || value.Any(char.IsControl))
            throw new ArgumentException("The Local Agent Run identity is invalid.", name);
    }

    private sealed record RunRoute(LocalAgentRunSnapshot Run, LocalAgentRunControlState State);
}
