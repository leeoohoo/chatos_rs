using ChatOS.Core.Domain;

namespace ChatOS.Presentation.Chat;

public sealed partial class ConversationSessionViewModel
{
    private long _promptRefreshGeneration;
    private readonly HashSet<string> _resolvedPromptIds = new(StringComparer.Ordinal);

    private bool IsCurrentPromptSession(string conversationId, CancellationToken token) =>
        !token.IsCancellationRequested && _sessionCancellation?.Token == token &&
        string.Equals(ConversationId, conversationId, StringComparison.Ordinal);

    private bool IsCurrentPromptRequest(string conversationId, CancellationToken token, long generation) =>
        IsCurrentPromptSession(conversationId, token) &&
        generation == Volatile.Read(ref _promptRefreshGeneration);

    private async Task LoadPromptsAsync(string conversationId, CancellationToken cancellationToken)
    {
        if (!IsCurrentPromptSession(conversationId, cancellationToken)) return;
        var generation = Interlocked.Increment(ref _promptRefreshGeneration);
        IReadOnlyList<AskUserPrompt> prompts;
        try
        {
            prompts = await _askUserService.FetchPromptsAsync(
                conversationId, cancellationToken: cancellationToken).ConfigureAwait(false);
        }
        catch (Exception) when (!IsCurrentPromptRequest(conversationId, cancellationToken, generation))
        {
            return;
        }

        if (!IsCurrentPromptRequest(conversationId, cancellationToken, generation)) return;
        await _dispatcher.InvokeAsync(() =>
        {
            if (!IsCurrentPromptRequest(conversationId, cancellationToken, generation)) return;
            ReconcilePrompts(prompts, conversationId, cancellationToken);
        }).ConfigureAwait(false);
    }

    private void ReconcilePrompts(
        IReadOnlyList<AskUserPrompt> prompts, string conversationId, CancellationToken token)
    {
        var existing = PendingPrompts.ToDictionary(static card => card.Id, StringComparer.Ordinal);
        foreach (var card in existing.Values.Where(static card => !card.Prompt.IsPending))
            _resolvedPromptIds.Add(card.Id);

        var desired = new List<AskUserPromptViewModel>();
        var seen = new HashSet<string>(StringComparer.Ordinal);
        foreach (var prompt in prompts)
        {
            if (!string.Equals(prompt.ConversationId, conversationId, StringComparison.Ordinal) ||
                !seen.Add(prompt.Id)) continue;
            existing.TryGetValue(prompt.Id, out var card);
            if (!prompt.IsPending)
            {
                _resolvedPromptIds.Add(prompt.Id);
                card?.Refresh(prompt);
                continue;
            }
            // Resolved IDs cannot become pending again because of an older server snapshot.
            if (_resolvedPromptIds.Contains(prompt.Id)) continue;

            if (card is not null && (card.IsSubmitting || card.HasSameInputs(prompt)))
            {
                if (card.HasSameInputs(prompt)) card.Refresh(prompt);
                desired.Add(card);
            }
            else
            {
                desired.Add(new AskUserPromptViewModel(
                    prompt, _askUserService,
                    () => LoadPromptsAsync(conversationId, token), _localization,
                    () => Interlocked.Increment(ref _promptRefreshGeneration)));
            }
        }

        // Keep unchanged card instances in place so native controls retain input and focus.
        for (var index = PendingPrompts.Count - 1; index >= 0; index--)
        {
            if (desired.Contains(PendingPrompts[index])) continue;
            PendingPrompts[index].Deactivate();
            PendingPrompts.RemoveAt(index);
        }
        for (var index = 0; index < desired.Count; index++)
        {
            if (index < PendingPrompts.Count && ReferenceEquals(PendingPrompts[index], desired[index])) continue;
            var previousIndex = PendingPrompts.IndexOf(desired[index]);
            if (previousIndex >= 0) PendingPrompts.Move(previousIndex, index);
            else PendingPrompts.Insert(index, desired[index]);
        }
    }
}
