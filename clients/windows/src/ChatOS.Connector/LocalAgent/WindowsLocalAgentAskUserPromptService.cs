using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

/// Ask User is projected only from durable Local Agent Run state. Prompt IDs
/// are the Runtime interaction IDs, and every mutation re-resolves the exact
/// account/thread/run/interaction tuple before it reaches the Host.
public sealed class WindowsLocalAgentAskUserPromptService(
    IWindowsLocalAgentProjectionStore store,
    IWindowsLocalAgentAccountSession accountSession,
    WindowsLocalAgentRunProjectionRefresher refresher) : IAskUserPromptService
{
    public async Task<IReadOnlyList<AskUserPrompt>> FetchPromptsAsync(
        string conversationId,
        int limit = 100,
        CancellationToken cancellationToken = default)
    {
        RequireIdentity(conversationId, nameof(conversationId));
        if (limit <= 0) throw new ArgumentOutOfRangeException(nameof(limit));
        var projection = await RequireProjectionAsync(cancellationToken).ConfigureAwait(false);
        var routes = PendingRoutes(projection, conversationId).ToArray();
        if (routes.Select(route => route.Prompt.Id).Distinct(StringComparer.Ordinal).Count()
            != routes.Length)
            throw new InvalidDataException(
                "The Local Agent projection contains duplicate pending interaction identities.");
        return routes.OrderBy(route => route.Prompt.CreatedAt)
            .ThenBy(route => route.Prompt.Id, StringComparer.Ordinal)
            .TakeLast(limit).Select(route => route.Prompt).ToArray();
    }

    public async Task<AskUserPrompt> SubmitAsync(
        string promptId,
        string conversationId,
        AskUserSubmission submission,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(submission);
        var (projection, route) = await ResolveRouteAsync(
            promptId, conversationId, cancellationToken).ConfigureAwait(false);
        var answer = BuildAnswer(route.Prompt, submission);
        var client = await accountSession.GetClientAsync(projection.AccountId, cancellationToken)
            .ConfigureAwait(false);
        _ = await client.AcceptAsync(
            LocalAgentCommand.AnswerUserQuestion(route.Run.RunId, route.Prompt.Id, answer),
            cancellationToken).ConfigureAwait(false);
        _ = await refresher.RefreshAsync(
            projection.AccountId, route.Run.RunId, cancellationToken).ConfigureAwait(false);
        return route.Prompt with { Status = AskUserPromptStatus.Ok, UpdatedAt = DateTimeOffset.UtcNow };
    }

    public async Task<AskUserPrompt> CancelAsync(
        string promptId,
        string conversationId,
        CancellationToken cancellationToken = default)
    {
        var (projection, route) = await ResolveRouteAsync(
            promptId, conversationId, cancellationToken).ConfigureAwait(false);
        if (!route.Prompt.AllowsCancel)
            throw new InvalidOperationException("This Local Agent question cannot be cancelled.");
        var client = await accountSession.GetClientAsync(projection.AccountId, cancellationToken)
            .ConfigureAwait(false);
        _ = await client.AcceptAsync(
            LocalAgentCommand.CancelRun(route.Run.RunId, route.Run.Version),
            cancellationToken).ConfigureAwait(false);
        _ = await refresher.RefreshAsync(
            projection.AccountId, route.Run.RunId, cancellationToken).ConfigureAwait(false);
        return route.Prompt with
        {
            Status = AskUserPromptStatus.Canceled,
            UpdatedAt = DateTimeOffset.UtcNow,
        };
    }

    private async Task<(WindowsLocalAgentProjectionSnapshot Projection, PendingRoute Route)>
        ResolveRouteAsync(
            string promptId,
            string conversationId,
            CancellationToken cancellationToken)
    {
        RequireIdentity(promptId, nameof(promptId));
        RequireIdentity(conversationId, nameof(conversationId));
        var projection = await RequireProjectionAsync(cancellationToken).ConfigureAwait(false);
        var matches = PendingRoutes(projection, conversationId)
            .Where(route => string.Equals(route.Prompt.Id, promptId, StringComparison.Ordinal))
            .ToArray();
        return matches.Length == 1
            ? (projection, matches[0])
            : throw new InvalidOperationException(
                "The Local Agent question is no longer pending for this conversation.");
    }

    private async Task<WindowsLocalAgentProjectionSnapshot> RequireProjectionAsync(
        CancellationToken cancellationToken) =>
        await store.GetAsync(cancellationToken).ConfigureAwait(false)
        ?? throw new InvalidOperationException("The Local Agent account projection is not available.");

    private static IEnumerable<PendingRoute> PendingRoutes(
        WindowsLocalAgentProjectionSnapshot projection,
        string conversationId)
    {
        foreach (var route in WindowsLocalAgentInteractionProjection.Routes(projection))
        {
            if (!string.Equals(route.Source.ThreadId, conversationId, StringComparison.Ordinal))
                continue;
            if (WindowsLocalAgentInteractionProjection.TryAskUserPrompt(route, out var prompt))
                yield return new PendingRoute(route.Run, prompt);
        }
    }

    private static LocalAgentUserAnswer BuildAnswer(
        AskUserPrompt prompt,
        AskUserSubmission submission)
    {
        var text = submission.Values.TryGetValue("answer", out var answer)
            && !string.IsNullOrWhiteSpace(answer) ? answer.Trim() : null;
        IReadOnlyList<string> selections = submission.Selection switch
        {
            AskUserSelection.Single single => [single.Value],
            AskUserSelection.Multiple multiple => multiple.Values,
            null => [],
            _ => throw new InvalidDataException("The Local Agent answer selection is invalid."),
        };
        if (selections.Any(string.IsNullOrWhiteSpace)
            || selections.Distinct(StringComparer.Ordinal).Count() != selections.Count)
            throw new InvalidDataException("The Local Agent answer contains invalid selections.");
        if (prompt.Choice is { } choice)
        {
            var allowed = choice.Options.Select(option => option.Value).ToHashSet(StringComparer.Ordinal);
            if (selections.Any(value => !allowed.Contains(value))
                || selections.Count < choice.MinimumSelectionCount
                || selections.Count > choice.MaximumSelectionCount)
                throw new InvalidDataException("The Local Agent answer does not satisfy the question options.");
        }
        else if (selections.Count != 0)
            throw new InvalidDataException(
                "A free-text Local Agent question cannot accept option selections.");
        if (text is null && selections.Count == 0)
            throw new InvalidDataException("The Local Agent answer is empty.");
        return new LocalAgentUserAnswer(text, selections, []);
    }

    private static void RequireIdentity(string value, string name)
    {
        if (string.IsNullOrWhiteSpace(value) || value != value.Trim() || value.Any(char.IsControl))
            throw new ArgumentException("The Local Agent question identity is invalid.", name);
    }

    private sealed record PendingRoute(LocalAgentRunSnapshot Run, AskUserPrompt Prompt);
}
