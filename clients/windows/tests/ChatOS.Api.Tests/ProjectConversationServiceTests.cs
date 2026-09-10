using System.Net;
using System.Text.Json;
using ChatOS.Api.Http;
using ChatOS.Api.Workspace;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Tests;

public sealed class ProjectConversationServiceTests
{
    [Fact]
    public async Task EnsureConversationOnlyUsesConversationsApi()
    {
        var requests = new List<(string Method, string Path, string? Query, string? Body)>();
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            requests.Add((
                request.Method.Method,
                request.RequestUri!.AbsolutePath,
                request.RequestUri.Query,
                request.Content?.ReadAsStringAsync().GetAwaiter().GetResult()));
            return (request.Method.Method, request.RequestUri.AbsolutePath) switch
            {
                ("GET", "/api/chatos/conversations") =>
                    StubHttpMessageHandler.Json("[]"),
                ("POST", "/api/chatos/conversations") =>
                    StubHttpMessageHandler.Json(
                        "{\"id\":\"conversation-1\",\"project_id\":\"project/1\"}",
                        HttpStatusCode.Created),
                _ => throw new InvalidOperationException(request.RequestUri.ToString()),
            };
        });
        var service = new ProjectConversationService(client);

        var conversationId = await service.EnsureConversationAsync(
            Project("project/1", "Windows App", "workspace", "project"),
            new WorkspaceContact("contact-1", "jiguli", "叽咕狸", "active"));

        Assert.Equal("conversation-1", conversationId);
        Assert.Equal(new[] { "GET", "POST" }, requests.Select(static value => value.Method));
        Assert.Equal("?project_id=project%2F1&limit=500&offset=0", requests[0].Query);

        using var document = JsonDocument.Parse(requests[1].Body!);
        var root = document.RootElement;
        Assert.Equal("project/1", root.GetProperty("project_id").GetString());
        Assert.Equal("叽咕狸", root.GetProperty("title").GetString());
        var metadata = root.GetProperty("metadata");
        Assert.Equal("contact-1", metadata.GetProperty("contact").GetProperty("contact_id").GetString());
        Assert.Equal("jiguli", metadata.GetProperty("chat_runtime").GetProperty("contact_agent_id").GetString());
        Assert.Equal("project/1", metadata.GetProperty("chat_runtime").GetProperty("project_context").GetProperty("projectId").GetString());
        Assert.NotEqual("-1", root.GetProperty("project_id").GetString());
    }

    [Fact]
    public async Task EnsureConversationUsesExistingConversationBeforeCreatingAnother()
    {
        var postCount = 0;
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            if (request.Method == HttpMethod.Post)
            {
                postCount++;
            }

            return request.RequestUri!.AbsolutePath switch
            {
                "/api/chatos/conversations" => StubHttpMessageHandler.Json(
                    """
                    [{
                      "id": "existing",
                      "project_id": "p1",
                      "message_count": 3,
                      "metadata": {
                        "contact": { "contact_id": "contact-1" },
                        "chat_runtime": {
                          "project_context": {
                            "schemaVersion": 1,
                            "projectId": "p1",
                            "projectName": "Project",
                            "projectRevision": 1,
                            "executionTarget": {
                              "deviceId": "device",
                              "workspaceId": "workspace",
                              "relativeRoot": "repo"
                            }
                          }
                        }
                      }
                    }]
                    """),
                _ => throw new InvalidOperationException(request.RequestUri.ToString()),
            };
        });
        var service = new ProjectConversationService(client);

        var conversationId = await service.EnsureConversationAsync(
            Project("p1", "Project", "workspace", "repo"),
            new WorkspaceContact("contact-1", "jiguli", "叽咕狸", null));

        Assert.Equal("existing", conversationId);
        Assert.Equal(0, postCount);
    }

    private static WorkspaceProject Project(string id, string name, string workspaceId, string relativeRoot)
    {
        var record = new LocalProjectRecord(
            id, "owner", new LocalProjectDraft(name, workspaceId, relativeRoot),
            1, LocalProjectStatus.Active, 1, 1);
        return new WorkspaceProject(
            id, name, $"local://connector/device/{workspaceId}/{relativeRoot}", relativeRoot, null,
            ProjectContextSnapshot.FromRecord(record, "device"));
    }
}
