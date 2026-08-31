using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Conversation;

public sealed class ConversationRuntimeSettingsService : IConversationRuntimeSettingsService
{
    private readonly ChatOSApiClient _client;

    public ConversationRuntimeSettingsService(ChatOSApiClient client)
    {
        _client = client;
    }

    public async Task<ConversationRuntimeSettings> FetchAsync(
        string conversationId,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.GetAsync<RuntimeSettingsDto>(
            $"conversations/{Uri.EscapeDataString(conversationId)}/runtime-settings",
            cancellationToken).ConfigureAwait(false);
        return response.ToDomain();
    }

    public async Task<IReadOnlyList<ConversationModelOption>> FetchAvailableModelsAsync(
        CancellationToken cancellationToken = default)
    {
        var response = await _client.GetAsync<IReadOnlyList<ModelConfigDto>>(
            "ai-model-configs",
            cancellationToken).ConfigureAwait(false);
        return response
            .Where(static config => config.Enabled != false && config.ModelName.TrimmedOrNull() is not null)
            .Select(static config => config.ToDomain())
            .ToArray();
    }

    public Task<ConversationRuntimeSettings> UpdateModelAsync(
        string conversationId,
        string modelId,
        CancellationToken cancellationToken = default) =>
        UpdateAsync(
            conversationId,
            new ModelUpdateDto(modelId),
            cancellationToken);

    public Task<ConversationRuntimeSettings> UpdatePlanModeAsync(
        string conversationId,
        bool enabled,
        CancellationToken cancellationToken = default) =>
        UpdateAsync(
            conversationId,
            new PlanModeUpdateDto(enabled),
            cancellationToken);

    public Task<ConversationRuntimeSettings> UpdateReasoningAsync(
        string conversationId,
        bool enabled,
        CancellationToken cancellationToken = default) =>
        UpdateAsync(
            conversationId,
            new ReasoningUpdateDto(enabled),
            cancellationToken);

    private async Task<ConversationRuntimeSettings> UpdateAsync(
        string conversationId,
        object body,
        CancellationToken cancellationToken)
    {
        var response = await _client.PutAsync<RuntimeSettingsDto>(
            $"conversations/{Uri.EscapeDataString(conversationId)}/runtime-settings",
            body,
            cancellationToken).ConfigureAwait(false);
        return response.ToDomain();
    }
}

internal sealed record ModelUpdateDto(
    [property: JsonPropertyName("selected_model_id")] string SelectedModelId);

internal sealed record PlanModeUpdateDto(
    [property: JsonPropertyName("plan_mode_enabled")] bool PlanModeEnabled);

internal sealed record ReasoningUpdateDto(
    [property: JsonPropertyName("reasoning_enabled")] bool ReasoningEnabled);
