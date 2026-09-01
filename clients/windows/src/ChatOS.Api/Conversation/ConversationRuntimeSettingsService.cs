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
        var seenIds = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        var seenDisplayModels = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        var models = new List<ConversationModelOption>();
        foreach (var config in response)
        {
            var modelName = config.ModelName.TrimmedOrNull();
            if (config.Enabled == false || modelName is null)
            {
                continue;
            }
            var id = config.Id.TrimmedOrNull();
            var displayName = config.Name.TrimmedOrNull() ?? modelName;
            var displayModelKey = $"{displayName}\0{modelName}";
            if (id is null || !seenIds.Add(id) || !seenDisplayModels.Add(displayModelKey))
            {
                continue;
            }
            models.Add(config.ToDomain());
        }
        return models;
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
