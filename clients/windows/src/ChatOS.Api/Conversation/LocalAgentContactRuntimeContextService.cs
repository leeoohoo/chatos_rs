using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Conversation;

public sealed class LocalAgentContactRuntimeContextService(ChatOSApiClient client)
    : ILocalAgentContactRuntimeContextService
{
    public async Task<LocalAgentContactRuntimeContext> FetchAsync(
        string agentId,
        CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(agentId);
        var response = await client.GetAsync<RuntimeContextDto>(
            $"agents/{Uri.EscapeDataString(agentId)}/runtime-context",
            cancellationToken).ConfigureAwait(false);
        return new LocalAgentContactRuntimeContext(
            response.AgentId,
            response.Name,
            Trimmed(response.Description),
            Trimmed(response.Category),
            response.RoleDefinition,
            response.Skills.Select(skill => new LocalAgentContactSkill(
                skill.Id,
                skill.Name,
                skill.Content)).ToArray(),
            response.UpdatedAt);
    }

    private static string? Trimmed(string? value) =>
        string.IsNullOrWhiteSpace(value) ? null : value.Trim();

    private sealed record RuntimeContextDto(
        [property: JsonPropertyName("agent_id")] string AgentId,
        [property: JsonPropertyName("name")] string Name,
        [property: JsonPropertyName("description")] string? Description,
        [property: JsonPropertyName("category")] string? Category,
        [property: JsonPropertyName("role_definition")] string RoleDefinition,
        [property: JsonPropertyName("skills")] IReadOnlyList<SkillDto> Skills,
        [property: JsonPropertyName("updated_at")] string UpdatedAt);

    private sealed record SkillDto(
        [property: JsonPropertyName("id")] string Id,
        [property: JsonPropertyName("name")] string Name,
        [property: JsonPropertyName("content")] string Content);
}
