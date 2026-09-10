using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;

namespace ChatOS.Api.Conversation;

public sealed class UserModelDefaultsService(ChatOSApiClient client) : IUserModelDefaultsService
{
    public async Task<UserModelDefaults> FetchAsync(CancellationToken cancellationToken = default)
    {
        var response = await client.GetAsync<UserModelDefaultsDto>(
            "ai-model-configs/settings",
            cancellationToken).ConfigureAwait(false);
        return new UserModelDefaults(Normalize(response.TaskRunnerDefaultModelConfigId));
    }

    public async Task<UserModelDefaults> UpdateTaskRunnerDefaultAsync(
        string? defaultModelConfigId,
        CancellationToken cancellationToken = default)
    {
        var response = await client.PutAsync<UserModelDefaultsDto>(
            "ai-model-configs/settings",
            new UpdateUserModelDefaultsDto(Normalize(defaultModelConfigId) ?? string.Empty),
            cancellationToken).ConfigureAwait(false);
        return new UserModelDefaults(Normalize(response.TaskRunnerDefaultModelConfigId));
    }

    private static string? Normalize(string? value) =>
        string.IsNullOrWhiteSpace(value) ? null : value.Trim();

    private sealed record UserModelDefaultsDto(
        [property: JsonPropertyName("task_runner_default_model_config_id")]
        string? TaskRunnerDefaultModelConfigId);

    private sealed record UpdateUserModelDefaultsDto(
        [property: JsonPropertyName("task_runner_default_model_config_id")]
        string TaskRunnerDefaultModelConfigId);
}
