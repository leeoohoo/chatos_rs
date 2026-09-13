namespace ChatOS.Core.Domain;

public sealed record ConversationRuntimeSettings(
    string? SelectedModelId,
    string? SelectedModelName,
    string? SelectedThinkingLevel,
    bool ReasoningEnabled);

public sealed record ConversationModelOption(
    string Id,
    string DisplayName,
    string ModelName,
    string? ThinkingLevel,
    bool TaskEnabled = true,
    bool HasApiKey = true);
