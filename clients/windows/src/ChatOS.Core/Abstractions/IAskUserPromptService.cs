using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IAskUserPromptService
{
    Task<IReadOnlyList<AskUserPrompt>> FetchPromptsAsync(
        string conversationId,
        int limit = 100,
        CancellationToken cancellationToken = default);

    Task<AskUserPrompt> SubmitAsync(
        string promptId,
        string conversationId,
        AskUserSubmission submission,
        CancellationToken cancellationToken = default);

    Task<AskUserPrompt> CancelAsync(
        string promptId,
        string conversationId,
        CancellationToken cancellationToken = default);
}
