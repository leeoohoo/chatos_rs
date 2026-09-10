using ChatOS.Api.Workspace;

namespace ChatOS.Api.Tests;

public sealed class WorkspaceServiceTests
{
    [Fact]
    public async Task RelationsUseOnlyContactsAndConversations()
    {
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var requests = new System.Collections.Concurrent.ConcurrentBag<string>();
        var client = ApiTestClient.Create(store, request =>
        {
            var path = request.RequestUri!.AbsolutePath;
            requests.Add(path);
            if (path is not ("/api/chatos/contacts" or "/api/chatos/conversations"))
                throw new InvalidOperationException("Unexpected project request: " + path);
            return StubHttpMessageHandler.Json("[]");
        });
        var result = await new WorkspaceService(client).FetchWorkspaceRelationsAsync();
        Assert.Empty(result.Contacts);
        Assert.Empty(result.Conversations);
        Assert.Equal(2, requests.Count);
    }

    [Fact]
    public async Task FetchRelationsMapsParallelResourceResponsesWithoutSentinelProjectId()
    {
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request => request.RequestUri?.AbsolutePath switch
        {
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
                    "project_id":"-1",
                    "metadata":"{\"source_metadata\":{\"contact\":{\"contact_id\":\"contact-1\",\"agent_id\":\"jiguli\"}}}"
                  }
                ]
                """),
            _ => throw new InvalidOperationException(request.RequestUri?.ToString()),
        });
        var service = new WorkspaceService(client);

        var workspace = await service.FetchWorkspaceRelationsAsync();

        Assert.Equal("叽咕狸", Assert.Single(workspace.Contacts).Name);
        Assert.Equal("p1", workspace.Conversations[0].ProjectId);
        Assert.Null(workspace.Conversations[1].ProjectId);
        Assert.Equal("contact-1", workspace.Conversations[1].ContactId);
        Assert.DoesNotContain(workspace.Conversations, conversation => conversation.ProjectId == "-1");
    }
}
