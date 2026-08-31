using System.Text.Json;
using ChatOS.Api.Projects;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Tests;

public sealed class ProjectExecutionServiceTests
{
    [Fact]
    public async Task FetchUsesPreciseExecutionIdentityInPathAndQuery()
    {
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            Assert.Equal(
                "/api/chatos/projects/project%201/requirements/requirement%2F1/execution-plan",
                request.RequestUri!.AbsolutePath);
            Assert.Contains("conversation_id=conversation%2F1", request.RequestUri.Query, StringComparison.Ordinal);
            Assert.Contains("execution_group_id=group%201", request.RequestUri.Query, StringComparison.Ordinal);
            return StubHttpMessageHandler.Json("""
                {
                  "found":true,
                  "project_id":"project 1",
                  "requirement_id":"requirement/1",
                  "conversation_id":"conversation/1",
                  "execution_group_id":"group 1",
                  "status":"failed",
                  "confirmation_status":"failed",
                  "has_started_runs":false,
                  "failure_kind":"planner_failed",
                  "failure_reason":"规划 Agent 调用失败"
                }
                """);
        });

        var result = await new ProjectExecutionService(client).FetchExecutionAsync(Identity());

        Assert.Equal("group 1", result?.ExecutionGroupId);
        Assert.Equal("规划 Agent 调用失败", result?.FailureReason);
    }

    [Fact]
    public async Task ConfirmSendsCompleteIdentityWithoutDiscardFlag()
    {
        JsonElement body = default;
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            Assert.Equal(
                "/api/chatos/projects/project%201/requirements/requirement%2F1/confirm-execution",
                request.RequestUri!.AbsolutePath);
            using var document = JsonDocument.Parse(request.Content!.ReadAsStringAsync().GetAwaiter().GetResult());
            body = document.RootElement.Clone();
            return StubHttpMessageHandler.Json("""
                {"success":true,"status":"accepted","execution_group_id":"group 1","task_ids":["task-1"],"root_task_ids":["task-1"]}
                """);
        });

        var result = await new ProjectExecutionService(client).ConfirmExecutionAsync(Identity());

        Assert.True(result.Success);
        Assert.Equal("group 1", body.GetProperty("execution_group_id").GetString());
        Assert.Equal("conversation/1", body.GetProperty("conversation_id").GetString());
        Assert.Equal("contact-1", body.GetProperty("contact_id").GetString());
        Assert.False(body.TryGetProperty("discard_tasks", out _));
        Assert.Equal("task-1", Assert.Single(result.RootTaskIds));
    }

    [Fact]
    public async Task StopExplicitlyDiscardsTasksForCapturedExecutionGroup()
    {
        JsonElement body = default;
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            Assert.Equal(
                "/api/chatos/projects/project%201/requirements/requirement%2F1/stop",
                request.RequestUri!.AbsolutePath);
            using var document = JsonDocument.Parse(request.Content!.ReadAsStringAsync().GetAwaiter().GetResult());
            body = document.RootElement.Clone();
            return StubHttpMessageHandler.Json("""
                {"success":true,"status":"stopped","execution_group_id":"group 1","discarded_tasks":true}
                """);
        });

        var result = await new ProjectExecutionService(client).StopExecutionAsync(Identity());

        Assert.True(body.GetProperty("discard_tasks").GetBoolean());
        Assert.Equal("group 1", body.GetProperty("execution_group_id").GetString());
        Assert.True(result.DiscardedTasks);
    }

    private static ProjectExecutionIdentity Identity() => new(
        "project 1",
        "requirement/1",
        "group 1",
        "conversation/1",
        "contact-1");
}
