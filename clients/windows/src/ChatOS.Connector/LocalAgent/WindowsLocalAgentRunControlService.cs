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
        var routes = Routes(projection, conversationId).Where(state => !state.IsTerminal)
            .OrderBy(state => state.UpdatedAt).ThenBy(state => state.RunId,
                StringComparer.Ordinal).ToArray();
        if (routes.Select(state => state.RunId).Distinct(StringComparer.Ordinal).Count()
            != routes.Length)
            throw new InvalidDataException("The Local Agent projection contains duplicate Run identities.");
        return routes;
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
            .Where(state => state.RunId == runId).ToArray();
        if (matches.Length != 1 || !allowed(matches[0]))
            throw new InvalidOperationException("The Local Agent Run does not allow this action.");
        var client = await accountSession.GetClientAsync(projection.AccountId, cancellationToken)
            .ConfigureAwait(false);
        _ = await client.AcceptAsync(command(matches[0]), cancellationToken)
            .ConfigureAwait(false);
        _ = await refresher.RefreshAsync(projection.AccountId, runId, cancellationToken)
            .ConfigureAwait(false);
    }

    private async Task<WindowsLocalAgentProjectionSnapshot> RequireProjectionAsync(
        CancellationToken cancellationToken) =>
        await store.GetAsync(cancellationToken).ConfigureAwait(false)
        ?? throw new InvalidOperationException("The Local Agent account projection is not available.");

    private static IEnumerable<LocalAgentRunControlState> Routes(
        WindowsLocalAgentProjectionSnapshot projection, string conversationId) =>
        WindowsLocalAgentInteractionProjection.Routes(projection)
            .Where(route => route.Source.ThreadId == conversationId)
            .Select(WindowsLocalAgentInteractionProjection.RunControl);

    private static void RequireIdentity(string value, string name)
    {
        if (string.IsNullOrWhiteSpace(value) || value != value.Trim() || value.Any(char.IsControl))
            throw new ArgumentException("The Local Agent Run identity is invalid.", name);
    }

}
