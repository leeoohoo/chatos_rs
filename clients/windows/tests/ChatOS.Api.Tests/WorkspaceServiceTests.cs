using ChatOS.Api.Workspace;

namespace ChatOS.Api.Tests;

public sealed class WorkspaceServiceTests
{
    [Fact]
    public async Task FetchWorkspaceMapsParallelResourceResponsesWithoutSentinelProjectId()
    {
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request => request.RequestUri?.AbsolutePath switch
        {
            "/api/chatos/projects" => StubHttpMessageHandler.Json("""
                [{"id":"p1","name":"ChatOS","root_path":"C:\\\\src\\\\chatos","latest_session_id":"c1"}]
                """),
            "/api/chatos/contacts" => StubHttpMessageHandler.Json("""
                [{"id":"contact-1","agent_id":"jiguli","agent_name_snapshot":"叽咕狸","status":"active"}]
                """),
            "/api/chatos/conversations" => StubHttpMessageHandler.Json("""
                [
                  {
                    "id":"c1",
                    "title":"Windows 客户端",
                    "message_count":12,
                    "updated_at":"2026-08-30T10:00:00Z",
                    "metadata":{"source_metadata":{"chat_runtime":{"project_id":"p1"}}}
                  },
                  {
                    "id":"c2",
                    "title":"叽咕狸",
                    "metadata":"{\"source_metadata\":{\"contact\":{\"contact_id\":\"contact-1\",\"agent_id\":\"jiguli\"}}}"
                  }
                ]
                """),
            _ => throw new InvalidOperationException(request.RequestUri?.ToString()),
        });
        var service = new WorkspaceService(client);

        var workspace = await service.FetchWorkspaceAsync();

        Assert.Single(workspace.Projects);
        Assert.Equal("叽咕狸", Assert.Single(workspace.Contacts).Name);
        Assert.Equal("p1", workspace.Conversations[0].ProjectId);
        Assert.Null(workspace.Conversations[1].ProjectId);
        Assert.Equal("contact-1", workspace.Conversations[1].ContactId);
        Assert.DoesNotContain(workspace.Conversations, conversation => conversation.ProjectId == "-1");
    }
}
