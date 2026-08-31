using System.Text.Json;
using ChatOS.Api.Projects;

namespace ChatOS.Api.Tests;

public sealed class ProjectRunServiceTests
{
    [Fact]
    public async Task CatalogAndDefaultTargetPreserveStableTargetIdentity()
    {
        JsonElement body = default;
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            if (request.Method == HttpMethod.Post)
            {
                using var document = JsonDocument.Parse(
                    request.Content!.ReadAsStringAsync().GetAwaiter().GetResult());
                body = document.RootElement.Clone();
            }

            return request.RequestUri!.AbsolutePath switch
            {
                "/api/chatos/projects/project%2F1/run/catalog" => StubHttpMessageHandler.Json("""
                    {
                      "project_id":"project/1",
                      "status":"ready",
                      "default_target_id":"web/main",
                      "targets":[{
                        "id":"web/main",
                        "label":"Web",
                        "kind":"npm",
                        "language":"typescript",
                        "cwd":"apps/web",
                        "command":"npm run dev",
                        "source":"package.json",
                        "is_default":true,
                        "entrypoint":"src/main.ts",
                        "manifest_path":"package.json",
                        "required_toolchains":["node","npm"]
                      }]
                    }
                    """),
                "/api/chatos/projects/project%2F1/run/default" => StubHttpMessageHandler.Json("""
                    {"project_id":"project/1","status":"ready","default_target_id":"web/main","targets":[]}
                    """),
                _ => throw new InvalidOperationException(request.RequestUri.ToString()),
            };
        });
        var service = new ProjectRunService(client);

        var catalog = await service.FetchCatalogAsync("project/1");
        var saved = await service.SetDefaultTargetAsync("project/1", "web/main");

        var target = Assert.Single(catalog.Targets);
        Assert.Equal("web/main", target.Id);
        Assert.Equal(new[] { "node", "npm" }, target.RequiredToolchains);
        Assert.Equal("web/main", saved.DefaultTargetId);
        Assert.Equal("web/main", body.GetProperty("target_id").GetString());
    }

    [Fact]
    public async Task EnvironmentMapsOptionsIssuesAndSendsCompleteDraft()
    {
        JsonElement body = default;
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            if (request.Method == HttpMethod.Put)
            {
                using var document = JsonDocument.Parse(
                    request.Content!.ReadAsStringAsync().GetAwaiter().GetResult());
                body = document.RootElement.Clone();
            }

            return StubHttpMessageHandler.Json("""
                {
                  "options_by_kind":{"node":[{"id":"node-22","kind":"node","label":"Node.js","version":"22","path":"C:\\node.exe","source":"path","is_default":true}]},
                  "config_files":[{"kind":"package","label":"package.json","path":"package.json","preview":"{}","source":"project"}],
                  "validation_issues":[{"kind":"warning","message":"缺少 PORT","target_id":"web","path":".env","hint":"设置 PORT"}],
                  "selected_toolchains":{"node":"node-22"},
                  "custom_toolchains":{"node":{"kind":"node","label":"Custom Node","path":"D:\\node.exe"}},
                  "env_vars":{"PORT":"3000"},
                  "terminal_ui_enabled":true
                }
                """);
        });
        var service = new ProjectRunService(client);

        var environment = await service.FetchEnvironmentAsync("project-1");
        await service.UpdateEnvironmentAsync(
            "project-1",
            new Dictionary<string, string> { ["node"] = "node-22" },
            new Dictionary<string, ChatOS.Core.Domain.ProjectRunCustomToolchain>
            {
                ["node"] = new("node", "Custom Node", "D:\\node.exe"),
            },
            new Dictionary<string, string> { ["PORT"] = "3100" });

        Assert.Equal("C:\\node.exe", Assert.Single(environment.ToolchainOptions["node"]).Path);
        Assert.Equal("web", Assert.Single(environment.ValidationIssues).TargetId);
        Assert.True(environment.TerminalUiEnabled);
        Assert.Equal("node-22", body.GetProperty("selected_toolchains").GetProperty("node").GetString());
        Assert.Equal("D:\\node.exe", body.GetProperty("custom_toolchains").GetProperty("node").GetProperty("path").GetString());
        Assert.Equal("3100", body.GetProperty("env_vars").GetProperty("PORT").GetString());
    }

    [Fact]
    public async Task StateAndMutationsUseTerminalInstanceIdentity()
    {
        var requests = new List<(HttpMethod Method, string Path, string? Body)>();
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            requests.Add((
                request.Method,
                request.RequestUri!.AbsolutePath,
                request.Content?.ReadAsStringAsync().GetAwaiter().GetResult()));
            if (request.RequestUri.AbsolutePath.EndsWith("/run/state", StringComparison.Ordinal))
            {
                return StubHttpMessageHandler.Json("""
                    {
                      "project_id":"project-1",
                      "status":"running",
                      "busy":false,
                      "running":true,
                      "instances":[{
                        "terminal_id":"terminal/42",
                        "terminal_name":"Web dev server",
                        "cwd":"apps/web",
                        "status":"running",
                        "busy":false,
                        "running":true,
                        "log":"ready",
                        "started_at":"2026-08-30T02:00:00Z"
                      }]
                    }
                    """);
            }

            return StubHttpMessageHandler.Json("{" + "\"success\":true,\"status\":\"accepted\"}");
        });
        var service = new ProjectRunService(client);

        var state = await service.FetchStateAsync("project-1");
        await service.StartAsync("project-1", "web/main");
        await service.StopAsync("terminal/42");
        await service.DeleteAsync("terminal/42");

        var instance = Assert.Single(state.Instances);
        Assert.Equal("terminal/42", instance.Id);
        Assert.Equal("ready", instance.Log);
        Assert.NotNull(instance.StartedAt);
        Assert.Contains(requests, value => value.Method == HttpMethod.Post && value.Path == "/api/chatos/terminals/terminal%2F42/interrupt");
        Assert.Contains(requests, value => value.Method == HttpMethod.Delete && value.Path == "/api/chatos/terminals/terminal%2F42");
        var start = Assert.Single(
            requests,
            value => value.Path.EndsWith("/run/execute", StringComparison.Ordinal));
        using var startBody = JsonDocument.Parse(start.Body!);
        Assert.Equal("web/main", startBody.RootElement.GetProperty("target_id").GetString());
        Assert.True(startBody.RootElement.GetProperty("create_if_missing").GetBoolean());
    }

    [Fact]
    public async Task AnalyzeUsesProjectPathEncoding()
    {
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            Assert.Equal(HttpMethod.Post, request.Method);
            Assert.Equal("/api/chatos/projects/project%2F1/run/analyze", request.RequestUri!.AbsolutePath);
            return StubHttpMessageHandler.Json("{" + "\"status\":\"ready\",\"targets\":[]}");
        });

        var result = await new ProjectRunService(client).AnalyzeAsync("project/1");

        Assert.Equal("project/1", result.ProjectId);
    }
}
