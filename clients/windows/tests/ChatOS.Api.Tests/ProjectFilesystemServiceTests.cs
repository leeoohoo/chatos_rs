using System.Text.Json;
using ChatOS.Api.Projects;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Tests;

public sealed class ProjectFilesystemServiceTests
{
    [Fact]
    public async Task FilesystemQueriesEncodeOpaqueConnectorPathsAndMapContent()
    {
        var paths = new List<string>();
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            paths.Add(request.RequestUri!.PathAndQuery);
            return request.RequestUri.AbsolutePath switch
            {
                "/api/chatos/fs/entries" => StubHttpMessageHandler.Json("""
                    {
                      "path":"local://connector/device/workspace/src",
                      "parent":"local://connector/device/workspace",
                      "writable":true,
                      "entries":[
                        {"name":"App.cs","path":"local://connector/device/workspace/src/App.cs","display_path":"src/App.cs","is_dir":false,"writable":true,"size":42,"modified_at":"2026-08-30T09:00:00Z"}
                      ],
                      "truncated":false
                    }
                    """),
                "/api/chatos/fs/read" => StubHttpMessageHandler.Json("""
                    {"path":"local://connector/device/workspace/src/App.cs","relative_path":"src/App.cs","name":"App.cs","content_type":"text/x-csharp","is_binary":false,"writable":true,"size":42,"content":"class App {}"}
                    """),
                _ => throw new InvalidOperationException(request.RequestUri.ToString()),
            };
        });
        var service = new ProjectFilesystemService(client);
        const string root = "local://connector/device/workspace/src";

        var listing = await service.ListEntriesAsync(root, true);
        var content = await service.ReadFileAsync($"{root}/App.cs");

        Assert.True(listing.IsWritable);
        Assert.Equal("src/App.cs", Assert.Single(listing.Entries).DisplayPath);
        Assert.Equal("class App {}", content.Content);
        Assert.Equal("src/App.cs", content.DisplayPath);
        Assert.Contains("path=local%3A%2F%2Fconnector%2Fdevice%2Fworkspace%2Fsrc", paths[0]);
        Assert.Contains("force_refresh=true", paths[0]);
    }

    [Fact]
    public async Task FilesystemMutationsSendExplicitPathsAndOperationModes()
    {
        var bodies = new List<(string Path, JsonElement Body)>();
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            using var document = JsonDocument.Parse(
                request.Content?.ReadAsStringAsync().GetAwaiter().GetResult() ?? "{}");
            bodies.Add((request.RequestUri!.AbsolutePath, document.RootElement.Clone()));
            return StubHttpMessageHandler.Json("{\"success\":true}");
        });
        var service = new ProjectFilesystemService(client);

        await service.WriteFileAsync("root/file.md", "updated");
        await service.CreateDirectoryAsync("root", "docs");
        var moved = await service.MoveEntryAsync("root/old.md", "root", "new.md");
        await service.DeleteEntryAsync("root/docs", true);
        await service.OpenExternallyAsync("root/file.md", ProjectFileExternalOpenMode.Code);

        Assert.Equal(new[]
        {
            "/api/chatos/fs/write",
            "/api/chatos/fs/mkdir",
            "/api/chatos/fs/move",
            "/api/chatos/fs/delete",
            "/api/chatos/fs/open",
        }, bodies.Select(static value => value.Path));
        Assert.Equal("updated", bodies[0].Body.GetProperty("content").GetString());
        Assert.Equal("root", bodies[1].Body.GetProperty("parent_path").GetString());
        Assert.Equal("root/old.md", bodies[2].Body.GetProperty("source_path").GetString());
        Assert.Equal("root", bodies[2].Body.GetProperty("target_parent_path").GetString());
        Assert.Equal("new.md", bodies[2].Body.GetProperty("target_name").GetString());
        Assert.False(bodies[2].Body.GetProperty("replace_existing").GetBoolean());
        Assert.True(bodies[3].Body.GetProperty("recursive").GetBoolean());
        Assert.Equal("code", bodies[4].Body.GetProperty("mode").GetString());
        Assert.Equal("root/old.md", moved.FromPath);
    }
}
