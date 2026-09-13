using System.Text.Json;
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
        {
            throw new InvalidDataException(
                "The Local Agent projection contains duplicate pending interaction identities.");
        }
        return routes
            .OrderBy(route => route.Prompt.CreatedAt)
            .ThenBy(route => route.Prompt.Id, StringComparer.Ordinal)
            .TakeLast(limit)
            .Select(route => route.Prompt)
            .ToArray();
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
        foreach (var recovered in projection.Runs.Values)
        {
            var source = WindowsLocalAgentRunSourceResolver.Resolve(projection, recovered);
            if (source is null
                || !string.Equals(source.ThreadId, conversationId, StringComparison.Ordinal)) continue;
            if (TryMapPrompt(recovered.Run, source.ThreadId, source.TurnId, out var prompt))
                yield return new PendingRoute(recovered.Run, prompt);
        }
    }

    private static bool TryMapPrompt(
        LocalAgentRunSnapshot run,
        string threadId,
        string turnId,
        out AskUserPrompt prompt)
    {
        prompt = null!;
        if (run.Status != LocalAgentRunStatus.Paused
            || run.PendingInteraction is not { ValueKind: JsonValueKind.Object } pending)
            return false;
        if (String(pending, "type") != "ask_user") return false;
        var interactionId = RequiredString(pending, "interaction_id");
        if (!pending.TryGetProperty("question", out var question)
            || question.ValueKind != JsonValueKind.Object)
            throw new InvalidDataException("The Local Agent question payload is invalid.");
        var message = RequiredString(question, "prompt");
        var options = RequiredArray(question, "options").EnumerateArray().Select(option =>
        {
            if (option.ValueKind != JsonValueKind.Object)
                throw new InvalidDataException("The Local Agent question option is invalid.");
            return new AskUserChoiceOption(
                RequiredString(option, "option_id"),
                RequiredString(option, "label"),
                OptionalString(option, "description"));
        }).ToArray();
        if (options.Select(option => option.Value).Distinct(StringComparer.Ordinal).Count()
            != options.Length)
            throw new InvalidDataException("The Local Agent question contains duplicate options.");
        var imageReferences = RequiredArray(question, "image_references")
            .EnumerateArray().Select(value => value.ValueKind == JsonValueKind.String
                ? ValidString(value.GetString(), "image reference")
                : throw new InvalidDataException("The Local Agent image reference is invalid."))
            .ToArray();
        var details = question.TryGetProperty("details", out var detailsValue)
            && detailsValue.ValueKind == JsonValueKind.Object ? detailsValue : default;
        var title = details.ValueKind == JsonValueKind.Object
            ? OptionalString(details, "title") ?? "需要你的确认"
            : "需要你的确认";
        var kind = details.ValueKind == JsonValueKind.Object
            ? OptionalString(details, "kind") ?? "local_agent"
            : "local_agent";
        var allowsCancel = details.ValueKind != JsonValueKind.Object
            || !details.TryGetProperty("allows_cancel", out var cancelValue)
            || cancelValue.ValueKind == JsonValueKind.True;
        var allowsMultiple = details.ValueKind == JsonValueKind.Object
            && details.TryGetProperty("allows_multiple", out var multipleValue)
            && multipleValue.ValueKind == JsonValueKind.True;
        prompt = new AskUserPrompt(
            interactionId,
            threadId,
            turnId,
            null,
            kind,
            AskUserPromptStatus.Pending,
            title,
            message,
            allowsCancel,
            null,
            options.Length == 0
                ? [new AskUserField(
                    "answer", "回复", null, "告诉 AI 你的决定或补充信息", string.Empty,
                    true, true, false)]
                : [],
            options.Length == 0 ? null : new AskUserChoice(
                allowsMultiple, options, [], 1, allowsMultiple ? options.Length : 1),
            run.UpdatedAt,
            run.UpdatedAt,
            imageReferences);
        return true;
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
        {
            throw new InvalidDataException("A free-text Local Agent question cannot accept option selections.");
        }
        if (text is null && selections.Count == 0)
            throw new InvalidDataException("The Local Agent answer is empty.");
        return new LocalAgentUserAnswer(text, selections, []);
    }

    private static JsonElement RequiredArray(JsonElement value, string property)
    {
        if (!value.TryGetProperty(property, out var item) || item.ValueKind != JsonValueKind.Array)
            throw new InvalidDataException($"The Local Agent question {property} is invalid.");
        return item;
    }

    private static string RequiredString(JsonElement value, string property) =>
        value.TryGetProperty(property, out var item) && item.ValueKind == JsonValueKind.String
            ? ValidString(item.GetString(), property)
            : throw new InvalidDataException($"The Local Agent question {property} is invalid.");

    private static string? OptionalString(JsonElement value, string property) =>
        value.TryGetProperty(property, out var item) && item.ValueKind == JsonValueKind.String
            ? item.GetString()?.Trim() is { Length: > 0 } result ? result : null
            : null;

    private static string? String(JsonElement value, string property) =>
        value.TryGetProperty(property, out var item) && item.ValueKind == JsonValueKind.String
            ? item.GetString()
            : null;

    private static string ValidString(string? value, string field) =>
        !string.IsNullOrWhiteSpace(value) && value == value.Trim() && !value.Any(char.IsControl)
            ? value
            : throw new InvalidDataException($"The Local Agent question {field} is invalid.");

    private static void RequireIdentity(string value, string name)
    {
        if (string.IsNullOrWhiteSpace(value) || value != value.Trim() || value.Any(char.IsControl))
            throw new ArgumentException("The Local Agent question identity is invalid.", name);
    }

    private sealed record PendingRoute(LocalAgentRunSnapshot Run, AskUserPrompt Prompt);
}
