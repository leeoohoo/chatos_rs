using ChatOS.Api.Conversation;

namespace ChatOS.Api.Tests;

public sealed class LocalAgentContactRuntimeContextServiceTests
{
    [Fact]
    public async Task FetchesTheExactEscapedAgentRuntimeAndMapsItsFrozenRevision()
    {
        string? requestedPath = null;
        var client = ApiTestClient.Create(new MemoryTokenStore(), request =>
        {
            requestedPath = request.RequestUri?.AbsolutePath;
            return StubHttpMessageHandler.Json("""
                {
                  "agent_id": "agent/design",
                  "name": "Design Agent",
                  "description": "  Visual specialist  ",
                  "category": "  design  ",
                  "role_definition": "Create polished interfaces",
                  "skills": [
                    {"id":"skill-1","name":"Layout","content":"Build visual hierarchy"}
                  ],
                  "updated_at": "revision-42"
                }
                """);
        });

        var context = await new LocalAgentContactRuntimeContextService(client)
            .FetchAsync("agent/design");

        Assert.Equal("/api/chatos/agents/agent%2Fdesign/runtime-context", requestedPath);
        Assert.Equal("agent/design", context.AgentId);
        Assert.Equal("Visual specialist", context.Description);
        Assert.Equal("design", context.Category);
        Assert.Equal("revision-42", context.Revision);
        Assert.Equal("skill-1", Assert.Single(context.Skills).Id);
    }
}
