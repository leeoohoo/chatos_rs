using System.Text.Json;
using ChatOS.Connector.Gateway;

namespace ChatOS.Connector.AgentTeams;

internal static class AgentTeamContextBudget
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    public static void EnsureWithinLimit(
        IReadOnlyList<object> input,
        IReadOnlyList<AgentToolDefinition> tools,
        NativeAgentRuntimeSettings settings)
    {
        var inputLimit = settings.ContextWindowTokens - settings.OutputReserveTokens;
        var estimatedTokens = Estimate(input, tools);
        if (estimatedTokens <= inputLimit) return;

        throw new AgentTeamException(AgentTeamError.ModelUnavailable,
            $"Agent input requires about {estimatedTokens} tokens, exceeding the managed " +
            $"context budget of {inputLimit} tokens after reserving output capacity.");
    }

    internal static int Estimate(
        IReadOnlyList<object> input,
        IReadOnlyList<AgentToolDefinition> tools)
    {
        var payload = new Dictionary<string, object>
        {
            ["input"] = input,
            ["tools"] = tools.Select(static tool => new Dictionary<string, object>
            {
                ["type"] = "function",
                ["name"] = tool.Name,
                ["description"] = tool.Description,
                ["parameters"] = tool.Parameters,
            }).ToArray(),
        };
        var bytes = JsonSerializer.SerializeToUtf8Bytes(payload, JsonOptions).Length;
        return Math.Max(1, (bytes + 3) / 4);
    }
}
