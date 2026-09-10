using System.Text.Json;
using ChatOS.Api.Tasks;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Tests;

public sealed class TaskRunnerHostServiceTests
{
    [Fact]
    public async Task PrepareBatchUsesTaskRunnerRouteAndCreatesDependenciesInTopologicalOrder()
    {
        var tokenStore = new MemoryTokenStore();
        tokenStore.Seed("valid");
        var createdBodies = new List<JsonElement>();
        var client = ApiTestClient.Create(tokenStore, request =>
        {
            Assert.Equal("/api/task/tasks", request.RequestUri?.AbsolutePath);
            if (request.Method == HttpMethod.Get)
            {
                Assert.Contains("project_id=project-1", request.RequestUri?.Query);
                Assert.Contains("tag=chatos-host-batch", Uri.UnescapeDataString(request.RequestUri?.Query ?? string.Empty));
                return StubHttpMessageHandler.Json("[]");
            }
            Assert.Equal(HttpMethod.Post, request.Method);
            using var body = JsonDocument.Parse(request.Content!.ReadAsStringAsync().GetAwaiter().GetResult());
            createdBodies.Add(body.RootElement.Clone());
            var value = body.RootElement;
            var id = value.GetProperty("title").GetString() == "Foundation" ? "task-foundation" : "task-feature";
            return StubHttpMessageHandler.Json(JsonSerializer.Serialize(new
            {
                id,
                title = value.GetProperty("title").GetString(),
                status = "ready",
                project_id = value.GetProperty("project_id").GetString(),
                input_payload = value.GetProperty("input_payload"),
                prerequisite_task_ids = value.GetProperty("prerequisite_task_ids"),
                default_model_config_id = value.GetProperty("default_model_config_id").GetString(),
                last_run_id = (string?)null,
                updated_at = "2026-09-09T00:00:00Z",
            }));
        });
        var service = new TaskRunnerHostService(client);
        var request = new PluginHostTaskBatchRequest("intent-1", [
            new("feature", "Feature", "Build feature", null, "Accepted", ["foundation"]),
            new("foundation", "Foundation", "Build foundation", null, null, []),
        ]);

        var result = await service.PrepareBatchAsync(
            request,
            Context(),
            new PluginHostIdentity("plugin-1", "studio", "release-1", "1.0.0", new string('a', 64)),
            "model-1");

        Assert.False(result.Reused);
        Assert.Equal(["Foundation", "Feature"], createdBodies.Select(body => body.GetProperty("title").GetString()));
        Assert.Empty(createdBodies[0].GetProperty("prerequisite_task_ids").EnumerateArray());
        Assert.Equal("task-foundation", createdBodies[1].GetProperty("prerequisite_task_ids")[0].GetString());
        Assert.All(createdBodies, body => Assert.Equal("project-1", body.GetProperty("project_id").GetString()));
        Assert.All(createdBodies, body => Assert.Equal("model-1", body.GetProperty("default_model_config_id").GetString()));
        Assert.Equal(["feature", "foundation"], result.Tasks.Select(task => task.ClientRef));
    }

    [Fact]
    public async Task PrepareBatchRejectsDependencyCycleBeforeCallingTaskRunner()
    {
        var calls = 0;
        var service = new TaskRunnerHostService(ApiTestClient.Create(
            new MemoryTokenStore(),
            _ =>
            {
                calls++;
                return StubHttpMessageHandler.Json("[]");
            }));
        var request = new PluginHostTaskBatchRequest("intent-1", [
            new("one", "One", "One", null, null, ["two"]),
            new("two", "Two", "Two", null, null, ["one"]),
        ]);

        await Assert.ThrowsAsync<InvalidOperationException>(() => service.PrepareBatchAsync(
            request,
            Context(),
            new PluginHostIdentity("plugin-1", "studio", "release-1", "1.0.0", new string('a', 64)),
            "model-1"));

        Assert.Equal(0, calls);
    }

    [Fact]
    public async Task StartBatchRevalidatesProjectTasksThenStartsThroughTaskRunner()
    {
        var calls = new List<string>();
        var service = new TaskRunnerHostService(ApiTestClient.Create(new MemoryTokenStore(), request =>
        {
            calls.Add($"{request.Method}:{request.RequestUri?.AbsolutePath}");
            if (request.Method == HttpMethod.Get)
            {
                Assert.Contains("project_id=project-1", request.RequestUri?.Query);
                return StubHttpMessageHandler.Json("""
                    [{"id":"task-1","title":"Task","status":"ready","project_id":"project-1","updated_at":"2026-09-09T00:00:00Z"}]
                    """);
            }
            using var body = JsonDocument.Parse(request.Content!.ReadAsStringAsync().GetAwaiter().GetResult());
            Assert.Equal("task-1", body.RootElement.GetProperty("task_ids")[0].GetString());
            return StubHttpMessageHandler.Json("""
                {"results":[{"task_id":"task-1","ok":true,"run_id":"run-1"}]}
                """);
        }));

        var result = await service.StartBatchAsync(["task-1"], "project-1");

        Assert.Equal([
            "GET:/api/task/tasks/summaries",
            "POST:/api/task/tasks/batch/runs",
        ], calls);
        Assert.True(Assert.Single(result).Ok);
        Assert.Equal("run-1", result[0].RunId);
    }

    private static ProjectContextSnapshot Context() => new(
        1,
        "project-1",
        "Project",
        3,
        new ProjectContextExecutionTarget("device-1", "workspace-1", "src"));
}
