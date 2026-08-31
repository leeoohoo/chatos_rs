using System.Text.Json;
using ChatOS.Api.Tasks;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Tests;

public sealed class MessageTaskGraphServiceTests
{
    [Fact]
    public async Task GraphUsesSourceLookupAndPreservesTaskTitlesStatusesAndDependencies()
    {
        Uri? captured = null;
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            captured = request.RequestUri;
            return StubHttpMessageHandler.Json("""
                {
                  "root_task_ids":["task-2"],
                  "source_session_id":"conversation-1",
                  "source_turn_id":"turn-1",
                  "source_user_message_id":"message-1",
                  "nodes":[
                    {"depth":0,"is_root":false,"is_current_message":true,"task":{"id":"task-1","title":"准备环境","status":"completed","prerequisite_task_ids":[]}},
                    {"depth":1,"is_root":true,"is_current_message":true,"task":{"id":"task-2","title":"实现 Windows 客户端","status":"blocked","last_run_id":"run-2","prerequisite_task_ids":["task-1"]}}
                  ],
                  "edges":[{"id":"task-1->task-2","source":"task-1","target":"task-2","kind":"prerequisite"}]
                }
                """);
        });
        var service = new MessageTaskGraphService(client);

        var graph = await service.FetchGraphAsync(
            "message/1",
            new MessageTaskLookup("conversation-1", "turn-1", "message/1"));

        Assert.Equal("实现 Windows 客户端", graph.Nodes[1].Task.Title);
        Assert.Equal("blocked", graph.Nodes[1].Task.Status);
        Assert.Equal("task-1", Assert.Single(graph.Nodes[1].Task.PrerequisiteTaskIds));
        Assert.Equal("/api/chatos/messages/message%2F1/task-runner/graph", captured?.AbsolutePath);
        Assert.Contains("session_id=conversation-1", captured?.Query);
        Assert.Contains("turn_id=turn-1", captured?.Query);
        Assert.Contains("source_user_message_id=message%2F1", captured?.Query);
    }

    [Fact]
    public async Task RunDetailUsesBoundedEventPageAndMapsStableRunIdentity()
    {
        Uri? captured = null;
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            captured = request.RequestUri;
            return StubHttpMessageHandler.Json("""
                {
                  "task":{"id":"task-2","title":"实现客户端","status":"running","last_run_id":"run-2","prerequisite_task_ids":[]},
                  "run":{"id":"run-2","task_id":"task-2","status":"running","report":{"content":"模型生成的主要结果"}},
                  "events":[{"id":"event-81","event_type":"thinking","message":"继续处理"}],
                  "events_total":121,
                  "events_has_more":true
                }
                """);
        });
        var service = new MessageTaskGraphService(client);

        var detail = await service.FetchRunAsync(
            "message-1",
            "run-2",
            new MessageTaskLookup("conversation-1", null, null),
            true,
            400,
            80);

        Assert.Equal("run-2", detail.Run.Id);
        Assert.Equal("task-2", detail.Run.TaskId);
        Assert.Equal("模型生成的主要结果", detail.Run.ReportContent);
        Assert.Equal(121, detail.EventsTotal);
        Assert.True(detail.EventsHasMore);
        Assert.Contains("event_limit=100", captured?.Query);
        Assert.Contains("event_offset=80", captured?.Query);
    }

    [Fact]
    public async Task TaskDetailPrefersExecutionIdentityFromInputPayload()
    {
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, _ => StubHttpMessageHandler.Json("""
            {
              "id":"task-2",
              "title":"实现客户端",
              "status":"completed",
              "default_model_config":{"id":"config-1","name":"主模型","provider":"openai","model":"gpt-5"},
              "last_run":{"id":"run-2","status":"completed","report":{"content":"完整模型输出"}},
              "prerequisite_tasks":[{"id":"task-1","title":"前置节点","status":"completed"}],
              "project_task_id":"stale-project-task",
              "input_payload":{"project_task_id":"project-task-1","execution_client_ref":"gateway-client","dependency_context_refs":["ctx-1"]},
              "mcp_config":{"workspace_dir":"C:\\src"}
            }
            """));
        var service = new MessageTaskGraphService(client);

        var task = await service.FetchTaskAsync(
            "message-1",
            "task-2",
            new MessageTaskLookup("conversation-1", null, null));

        Assert.Equal("主模型 · openai/gpt-5", task.DefaultModelConfig?.DisplayName);
        Assert.Equal("完整模型输出", task.LastRun?.ReportContent);
        Assert.Equal("前置节点", Assert.Single(task.PrerequisiteTasks).Title);
        Assert.Equal("project-task-1", task.ProjectTaskId);
        Assert.Equal("gateway-client", task.ExecutionClientRef);
        Assert.Equal("ctx-1", Assert.Single(task.DependencyContextRefs));
        Assert.Contains("workspace_dir", task.McpConfigJson);
    }

    [Fact]
    public async Task CancelTaskUsesScopedEndpointAndTrimmedReason()
    {
        string? body = null;
        Uri? captured = null;
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            captured = request.RequestUri;
            body = request.Content?.ReadAsStringAsync().GetAwaiter().GetResult();
            return StubHttpMessageHandler.Json("{\"success\":true}");
        });
        var service = new MessageTaskGraphService(client);

        await service.CancelTaskAsync(
            "message-1",
            "task-2",
            new MessageTaskLookup("conversation-1", "turn-1", null),
            "  用户取消  ");

        Assert.Equal(
            "/api/chatos/messages/message-1/task-runner/tasks/task-2/cancel",
            captured?.AbsolutePath);
        using var document = JsonDocument.Parse(body!);
        Assert.Equal("用户取消", document.RootElement.GetProperty("reason").GetString());
    }
}
