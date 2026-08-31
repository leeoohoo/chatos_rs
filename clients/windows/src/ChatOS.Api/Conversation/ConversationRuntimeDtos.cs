using System.Text.Json.Serialization;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Conversation;

internal sealed record RuntimeSettingsDto
{
    [JsonPropertyName("selected_model_id")]
    public string? SelectedModelId { get; init; }

    [JsonPropertyName("selected_model_name")]
    public string? SelectedModelName { get; init; }

    [JsonPropertyName("selected_thinking_level")]
    public string? SelectedThinkingLevel { get; init; }

    [JsonPropertyName("remote_connection_id")]
    public string? RemoteConnectionId { get; init; }

    [JsonPropertyName("workspace_root")]
    public string? WorkspaceRoot { get; init; }

    [JsonPropertyName("reasoning_enabled")]
    public bool ReasoningEnabled { get; init; }

    [JsonPropertyName("plan_mode_enabled")]
    public bool PlanModeEnabled { get; init; }

    public ConversationRuntimeSettings ToDomain() => new(
        SelectedModelId,
        SelectedModelName,
        SelectedThinkingLevel,
        ReasoningEnabled,
        PlanModeEnabled);
}

internal sealed record ModelConfigDto
{
    [JsonPropertyName("id")]
    public required string Id { get; init; }

    [JsonPropertyName("name")]
    public required string Name { get; init; }

    [JsonPropertyName("provider")]
    public string? Provider { get; init; }

    [JsonPropertyName("model")]
    public string? Model { get; init; }

    [JsonPropertyName("model_name")]
    public string? ModelNameValue { get; init; }

    [JsonPropertyName("thinking_level")]
    public string? ThinkingLevel { get; init; }

    [JsonPropertyName("temperature")]
    public double? Temperature { get; init; }

    [JsonPropertyName("enabled")]
    public bool? Enabled { get; init; }

    public string ModelName =>
        ModelNameValue.TrimmedOrNull() ?? Model.TrimmedOrNull() ?? Name;

    public ConversationModelOption ToDomain() => new(
        Id,
        Name.TrimmedOrNull() ?? ModelName,
        ModelName,
        ThinkingLevel.TrimmedOrNull());
}

internal static class ConversationStringExtensions
{
    public static string? TrimmedOrNull(this string? value)
    {
        value = value?.Trim();
        return string.IsNullOrEmpty(value) ? null : value;
    }
}
