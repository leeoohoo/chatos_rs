using System.Net;
using System.Text.Json;
using ChatOS.Api.Notepad;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Tests;

public sealed class NotepadServiceTests
{
    [Fact]
    public async Task ListsFoldersAndSearchesNotesWithBoundedLimit()
    {
        var requests = new List<HttpRequestMessage>();
        var client = ApiTestClient.Create(new MemoryTokenStore(), request =>
        {
            requests.Add(Clone(request));
            return request.RequestUri!.AbsolutePath.EndsWith("/notepad/folders", StringComparison.Ordinal)
                ? StubHttpMessageHandler.Json("""{"folders":["","项目/设计"]}""")
                : StubHttpMessageHandler.Json("""{"notes":[{"id":"note/1","title":"架构草稿","folder":"项目/设计","tags":["架构"],"updated_at":"2026-08-27T02:00:00Z"}]}""");
        });
        var service = new NotepadService(client);

        var folders = await service.ListFoldersAsync();
        var notes = await service.ListNotesAsync("架构 设计", 900);

        Assert.Equal(new[] { "", "项目/设计" }, folders);
        Assert.Equal("架构草稿", Assert.Single(notes).Title);
        var query = requests.Single(value => value.RequestUri!.AbsolutePath.EndsWith("/notepad/notes", StringComparison.Ordinal)).RequestUri!.Query;
        Assert.Contains("limit=500", query);
        Assert.Contains("recursive=true", query);
        Assert.Contains("query=%E6%9E%B6%E6%9E%84%20%E8%AE%BE%E8%AE%A1", query);
    }

    [Fact]
    public async Task CreateUpdateAndDeletePreservePayloadAndEncodedIdentity()
    {
        var requests = new List<(HttpMethod Method, Uri Uri, string Body)>();
        var client = ApiTestClient.Create(new MemoryTokenStore(), request =>
        {
            requests.Add((request.Method, request.RequestUri!, request.Content?.ReadAsStringAsync().Result ?? string.Empty));
            if (request.Method is { Method: "POST" } or { Method: "PATCH" } &&
                request.RequestUri!.AbsolutePath.Contains("/notepad/notes", StringComparison.Ordinal))
            {
                return StubHttpMessageHandler.Json("""{"note":{"id":"note/1","title":"架构方案","folder":"项目/设计","tags":["架构"]},"content":"# 定稿"}""");
            }

            return StubHttpMessageHandler.Json("""{"ok":true}""");
        });
        var service = new NotepadService(client);

        await service.CreateNoteAsync(new NotepadNoteDraft("项目/设计", "架构草稿", "# 初稿", ["架构"]));
        await service.UpdateNoteAsync("note/1", new NotepadNoteUpdate("架构方案", "# 定稿", Tags: ["架构"]));
        await service.DeleteFolderAsync("项目/新目录", recursive: true);
        await service.DeleteNoteAsync("note/1");

        var create = requests.First(value => value.Method == HttpMethod.Post);
        using var createJson = JsonDocument.Parse(create.Body);
        Assert.Equal("项目/设计", createJson.RootElement.GetProperty("folder").GetString());
        Assert.Equal("# 初稿", createJson.RootElement.GetProperty("content").GetString());
        Assert.Contains("/notepad/notes/note%2F1", requests.First(value => value.Method == HttpMethod.Patch).Uri.AbsoluteUri);
        Assert.Contains("recursive=true", requests.First(value => value.Uri.AbsolutePath.EndsWith("/notepad/folders", StringComparison.Ordinal)).Uri.Query);
    }

    private static HttpRequestMessage Clone(HttpRequestMessage request) => new(
        request.Method,
        request.RequestUri);
}
