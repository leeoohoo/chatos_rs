using System.Net;
using System.Text.Json;
using ChatOS.Api.Workspace;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Tests;

public sealed class WorkspaceResourceCreationServiceTests
{
    [Fact]
    public async Task EnsureConversationBindsContactAndCreatesMetadataWithoutSentinelIds()
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
                ("GET", "/api/chatos/projects/project%2F1/contacts") =>
                    StubHttpMessageHandler.Json("[]"),
                ("POST", "/api/chatos/projects/project%2F1/contacts") =>
                    StubHttpMessageHandler.Json("{\"contact_id\":\"contact-1\"}"),
                ("GET", "/api/chatos/conversations") =>
                    StubHttpMessageHandler.Json("[]"),
                ("POST", "/api/chatos/conversations") =>
                    StubHttpMessageHandler.Json(
                        "{\"id\":\"conversation-1\",\"project_id\":\"project/1\"}",
                        HttpStatusCode.Created),
                _ => throw new InvalidOperationException(request.RequestUri.ToString()),
            };
        });
        var service = new WorkspaceResourceCreationService(client);

        var conversationId = await service.EnsureConversationAsync(
            new WorkspaceProject(
                "project/1",
                "Windows App",
                "local://connector/device/workspace/project",
                "project",
                null),
            new WorkspaceContact("contact-1", "jiguli", "叽咕狸", "active"));

        Assert.Equal("conversation-1", conversationId);
        Assert.Equal(new[] { "GET", "POST", "GET", "POST" }, requests.Select(static value => value.Method));
        Assert.Equal("?project_id=project%2F1&limit=500&offset=0", requests[2].Query);

        using var document = JsonDocument.Parse(requests[3].Body!);
        var root = document.RootElement;
        Assert.Equal("project/1", root.GetProperty("project_id").GetString());
        Assert.Equal("叽咕狸", root.GetProperty("title").GetString());
        var metadata = root.GetProperty("metadata");
        Assert.Equal("contact-1", metadata.GetProperty("contact").GetProperty("contact_id").GetString());
        Assert.Equal("jiguli", metadata.GetProperty("chat_runtime").GetProperty("contact_agent_id").GetString());
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
                "/api/chatos/projects/p1/contacts" => StubHttpMessageHandler.Json(
                    "[{\"contact_id\":\"contact-1\",\"latest_session_id\":null}]"),
                "/api/chatos/conversations" => StubHttpMessageHandler.Json(
                    "[{\"id\":\"existing\",\"project_id\":\"p1\",\"message_count\":3," +
                    "\"metadata\":{\"contact\":{\"contact_id\":\"contact-1\"}}}]"),
                _ => throw new InvalidOperationException(request.RequestUri.ToString()),
            };
        });
        var service = new WorkspaceResourceCreationService(client);

        var conversationId = await service.EnsureConversationAsync(
            new WorkspaceProject("p1", "Project", "C:\\src", "C:\\src", null),
            new WorkspaceContact("contact-1", "jiguli", "叽咕狸", null));

        Assert.Equal("existing", conversationId);
        Assert.Equal(0, postCount);
    }
}
