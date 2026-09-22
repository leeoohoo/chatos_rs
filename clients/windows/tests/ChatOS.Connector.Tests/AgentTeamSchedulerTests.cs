using System.Net;
using System.Text;
using System.Text.RegularExpressions;
using ChatOS.Api.Http;
using ChatOS.Connector.AgentTeams;
using ChatOS.Connector.Persistence;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class AgentTeamSchedulerTests : IAsyncLifetime
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(), "chatos-agent-scheduler-tests", Guid.NewGuid().ToString("N"));
    private SqliteAgentTeamStore _store = null!;

    public async Task InitializeAsync()
    {
        var database = new LocalStateDatabase(Path.Combine(_directory, "state.db"));
        await database.InitializeAsync();
        _store = new SqliteAgentTeamStore(database);
    }

    public Task DisposeAsync()
    {
        if (Directory.Exists(_directory)) Directory.Delete(_directory, recursive: true);
        return Task.CompletedTask;
    }

    [Fact]
    public async Task GatewayUsesResponsesEndpointAndDecodesToolCalls()
    {
        HttpRequestMessage? captured = null;
        string? requestBody = null;
        var gateway = CreateGateway(async request =>
        {
            captured = request;
            requestBody = request.Content is null ? null : await request.Content.ReadAsStringAsync();
            return Json("""
                {"status":"completed","output":[
                  {"type":"message","content":[{"type":"output_text","text":"先检查"}]},
                  {"type":"function_call","call_id":"call-1","name":"todo_list","arguments":"{}"}
                ]}
                """);
        });
        var turn = await gateway.CompleteAsync(Profile(),
            [new Dictionary<string, object> { ["role"] = "user", ["content"] = "hello" }],
            AgentTeamToolExecutor.Definitions, CancellationToken.None);

        Assert.Equal("https://provider.example/v1/responses", captured!.RequestUri!.AbsoluteUri);
        Assert.Equal("Bearer", captured.Headers.Authorization!.Scheme);
        Assert.Equal("provider-secret", captured.Headers.Authorization.Parameter);
        Assert.Contains("\"store\":false", requestBody, StringComparison.Ordinal);
        Assert.DoesNotContain("provider-secret", requestBody, StringComparison.Ordinal);
        Assert.Equal("先检查", turn.Content);
        var call = Assert.Single(turn.ToolCalls);
        Assert.Equal("call-1", call.Id);
        Assert.Equal("todo_list", call.Name);
    }

    [Fact]
    public async Task SchedulerCompletesDeliveryPersistsReplyAndRun()
    {
        var profile = await _store.CreateAgentAsync("alice", Profile().Draft);
        var room = await _store.CreateRoomAsync("alice", "project-1",
            new("团队", "完成工作"), profile.Id);
        await CompleteInitialMaintenanceAsync();
        var post = await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Human, null, "开始"));
        var delivery = Assert.Single(post.Deliveries);
        var gateway = CreateGateway(_ => Task.FromResult(Json("""
            {"status":"completed","output":[
              {"type":"message","content":[{"type":"output_text","text":"工作已完成。"}]}
            ]}
            """)));
        var tools = new AgentTeamToolExecutor(_store, null!);
        var scheduler = new AgentTeamScheduler(_store, gateway, tools);

        await scheduler.DrainAsync("alice");

        Assert.Null(await _store.ClaimNextDeliveryAsync("alice"));
        var messages = await _store.ListMessagesAsync("alice", room.Id);
        var reply = Assert.Single(messages, value => value.SenderKind == AgentMessageSenderKind.Agent);
        Assert.Equal("工作已完成。", reply.Content);
        Assert.Equal(post.Message.Id, reply.RootMessageId);
        var run = Assert.Single(await _store.ListRunsAsync("alice", room.Id));
        Assert.Equal(delivery.Id, run.DeliveryId);
        Assert.Equal(AgentRunStatus.Completed, run.Status);
        Assert.Equal(1, run.ModelCalls);
    }

    [Fact]
    public async Task FailedTodoRetryResumesTheSameDurableRun()
    {
        var profile = await _store.CreateAgentAsync("alice", Profile().Draft);
        var room = await _store.CreateRoomAsync("alice", "project-1",
            new("团队", "恢复失败任务"), profile.Id);
        await CompleteInitialMaintenanceAsync();
        var todo = await _store.CreateTodoAsync("alice",
            new(room.Id, profile.Id, "重试任务"));
        var providerCalls = 0;
        long retryRevision = 0;
        var gateway = CreateGateway(async request =>
        {
            providerCalls++;
            if (providerCalls == 1) return Json("not-json");
            var body = await request.Content!.ReadAsStringAsync();
            var todoReference = Regex.Match(body, "todo_[a-f0-9]{32}").Value;
            Assert.NotEmpty(todoReference);
            return Json($$"""
                {"status":"completed","output":[
                  {"type":"function_call","call_id":"complete-retry","name":"todo_complete",
                   "arguments":"{\"todo_ref\":\"{{todoReference}}\",\"expected_revision\":{{retryRevision}},\"summary\":\"恢复完成\"}"}
                ]}
                """);
        });
        var scheduler = new AgentTeamScheduler(_store, gateway,
            new AgentTeamToolExecutor(_store, null!));

        await scheduler.DrainExecutorAsync("alice");

        var failedRun = Assert.Single(await _store.ListRunsAsync("alice", room.Id));
        Assert.Equal(AgentRunStatus.Failed, failedRun.Status);
        Assert.Equal(1, failedRun.ModelCalls);
        var blocked = Assert.IsType<AgentTodo>(await _store.GetTodoAsync("alice", todo.Id));
        Assert.Equal(AgentTodoStatus.Blocked, blocked.Status);
        var retried = await _store.UpdateTodoAsync("alice", todo.Id, blocked.Revision,
            AgentTodoStatus.Ready, "重试");
        retryRevision = retried.Revision;

        await scheduler.DrainExecutorAsync("alice");

        var completedRun = Assert.Single(await _store.ListRunsAsync("alice", room.Id));
        Assert.Equal(failedRun.Id, completedRun.Id);
        Assert.Equal(AgentRunStatus.Completed, completedRun.Status);
        Assert.Equal(2, completedRun.ModelCalls);
        Assert.Equal(2, providerCalls);
        Assert.Equal(AgentTodoStatus.Completed,
            (await _store.GetTodoAsync("alice", todo.Id))!.Status);
    }

    [Fact]
    public async Task GatewaySanitizesProviderFailures()
    {
        var gateway = CreateGateway(_ => Task.FromResult(Json(
            "{\"error\":{\"message\":\"secret upstream dump\",\"code\":\"bad_key\"}}",
            HttpStatusCode.Unauthorized)));

        var error = await Assert.ThrowsAsync<AgentTeamException>(() => gateway.CompleteAsync(
            Profile(), [], [], CancellationToken.None));

        Assert.Equal(AgentTeamError.ModelUnavailable, error.Code);
        Assert.Contains("rejected the configured credential", error.Message, StringComparison.Ordinal);
        Assert.Contains("bad_key", error.Message, StringComparison.Ordinal);
        Assert.DoesNotContain("secret upstream dump", error.Message, StringComparison.Ordinal);
    }

    [Fact]
    public async Task TransientProviderFailureRetriesWithinTheSameTodoRun()
    {
        var profile = await _store.CreateAgentAsync("alice", Profile().Draft);
        var room = await _store.CreateRoomAsync("alice", "project-1",
            new("团队", "重试瞬时失败"), profile.Id);
        await CompleteInitialMaintenanceAsync();
        var todo = await _store.CreateTodoAsync("alice",
            new(room.Id, profile.Id, "处理瞬时失败"));
        var providerCalls = 0;
        var gateway = CreateGateway(async request =>
        {
            providerCalls++;
            if (providerCalls == 1)
                return Json("{\"error\":{\"code\":\"temporary\"}}",
                    HttpStatusCode.ServiceUnavailable);
            var body = await request.Content!.ReadAsStringAsync();
            var todoReference = Regex.Match(body, "todo_[a-f0-9]{32}").Value;
            return Json($$"""
                {"status":"completed","output":[
                  {"type":"function_call","call_id":"complete-transient","name":"todo_complete",
                   "arguments":"{\"todo_ref\":\"{{todoReference}}\",\"expected_revision\":2,\"summary\":\"完成\"}"}
                ]}
                """);
        });
        var scheduler = new AgentTeamScheduler(_store, gateway,
            new AgentTeamToolExecutor(_store, null!));

        await scheduler.DrainExecutorAsync("alice");

        var run = Assert.Single(await _store.ListRunsAsync("alice", room.Id));
        Assert.Equal(AgentRunStatus.Completed, run.Status);
        Assert.Equal(2, run.ModelCalls);
        Assert.Equal(2, providerCalls);
        Assert.Equal(AgentTodoStatus.Completed,
            (await _store.GetTodoAsync("alice", todo.Id))!.Status);
    }

    [Fact]
    public async Task TodoCompletionDoesNotCancelItsActiveDelivery()
    {
        var profile = await _store.CreateAgentAsync("alice", Profile().Draft);
        var room = await _store.CreateRoomAsync("alice", "project-1",
            new("团队", "完成工作"), profile.Id);
        await CompleteInitialMaintenanceAsync();
        var source = await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Agent, profile.Id, "SOURCE_ALLOWED_FOR_EXECUTOR"));
        _ = await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Agent, profile.Id, "PRIVATE_MANAGER_CHAT_MUST_NOT_LEAK"));
        var asset = await _store.UpsertAssetAsync("alice", room.Id, null, profile.Id,
            AgentTeamAssetCategory.Plan, "executor plan", "EXECUTOR_ASSET_V1", null);
        var todo = await _store.CreateTodoAsync("alice",
            new(room.Id, profile.Id, "实现功能", SourceMessageId: source.Message.Id,
                ExecutionContract: new AgentTodoExecutionContract(
                    "EXECUTOR_OBJECTIVE", "isolated scope", ["verified output"],
                    ["tests pass"])));
        _ = await _store.UpsertAssetAsync("alice", room.Id, asset.Id, profile.Id,
            AgentTeamAssetCategory.Plan, "executor plan", "EXECUTOR_ASSET_V2", asset.Revision);
        var gateway = CreateGateway(async request =>
        {
            var body = await request.Content!.ReadAsStringAsync();
            Assert.Contains("EXECUTOR_OBJECTIVE", body, StringComparison.Ordinal);
            Assert.Contains("SOURCE_ALLOWED_FOR_EXECUTOR", body, StringComparison.Ordinal);
            Assert.DoesNotContain("PRIVATE_MANAGER_CHAT_MUST_NOT_LEAK", body,
                StringComparison.Ordinal);
            Assert.Contains("EXECUTOR_ASSET_V1", body, StringComparison.Ordinal);
            Assert.DoesNotContain("EXECUTOR_ASSET_V2", body, StringComparison.Ordinal);
            Assert.Contains("project_read", body, StringComparison.Ordinal);
            Assert.DoesNotContain("project_write", body, StringComparison.Ordinal);
            Assert.DoesNotContain("terminal_exec", body, StringComparison.Ordinal);
            Assert.DoesNotContain("chat_read_all_unread", body, StringComparison.Ordinal);
            var todoReference = Regex.Match(body, "todo_[a-f0-9]{32}").Value;
            Assert.NotEmpty(todoReference);
            return Json($$"""
                {"status":"completed","output":[
                  {"type":"function_call","call_id":"call-complete","name":"todo_complete",
                   "arguments":"{\"todo_ref\":\"{{todoReference}}\",\"expected_revision\":2,\"summary\":\"完成\"}"}
                ]}
                """);
        });
        var scheduler = new AgentTeamScheduler(_store, gateway,
            new AgentTeamToolExecutor(_store, null!));

        await scheduler.DrainAsync("alice");

        var completed = await _store.GetTodoAsync("alice", todo.Id);
        Assert.Equal(AgentTodoStatus.Completed, completed!.Status);
        Assert.Equal(AgentRunStatus.Completed,
            Assert.Single(await _store.ListRunsAsync("alice", room.Id)).Status);
    }

    [Fact]
    public async Task TodoStateToolsAreIsolatedByDeliveryLane()
    {
        var manager = await _store.CreateAgentAsync("alice",
            Profile().Draft with { Name = "Manager" });
        var worker = await _store.CreateAgentAsync("alice",
            Profile().Draft with { Name = "Worker" });
        var room = await _store.CreateRoomAsync("alice", "project-1",
            new("团队", "完成工作"), manager.Id);
        await CompleteInitialMaintenanceAsync();
        var workerMember = await _store.UpsertMemberAsync("alice", room.Id, worker.Id,
            new("developer", "实现任务"));
        var managerMember = Assert.Single(await _store.ListMembersAsync("alice", room.Id),
            value => value.AgentId == manager.Id);
        var todo = await _store.CreateTodoAsync("alice",
            new(room.Id, worker.Id, "隔离状态写入"));
        var executorDelivery = Assert.IsType<AgentDelivery>(
            await _store.ClaimNextDeliveryAsync("alice"));
        Assert.Equal(AgentDeliveryTrigger.Todo, executorDelivery.Trigger);
        todo = Assert.IsType<AgentTodo>(await _store.GetTodoAsync("alice", todo.Id));

        var tools = new AgentTeamToolExecutor(_store, null!);
        var executorDefinitions = tools.AllDefinitions(worker, room, executorDelivery, todo);
        Assert.Contains(executorDefinitions, value => value.Name == "todo_complete");
        Assert.Contains(executorDefinitions, value => value.Name == "todo_block");
        Assert.Contains(executorDefinitions, value => value.Name == "todo_progress");
        Assert.DoesNotContain(executorDefinitions, value => value.Name == "todo_update");

        var communication = executorDelivery with
        {
            Id = "communication-delivery",
            TargetAgentId = manager.Id,
            Trigger = AgentDeliveryTrigger.Mention,
            DeduplicationKey = "mention:manager",
        };
        var managerDefinitions = tools.AllDefinitions(manager, room, communication);
        Assert.Contains(managerDefinitions, value => value.Name == "todo_update");
        Assert.DoesNotContain(managerDefinitions, value => value.Name == "todo_complete");
        Assert.DoesNotContain(managerDefinitions, value => value.Name == "todo_block");
        Assert.DoesNotContain(managerDefinitions, value => value.Name == "todo_progress");
        Assert.DoesNotContain(tools.AllDefinitions(worker, room,
            communication with { TargetAgentId = worker.Id }),
            value => value.Name == "todo_update");

        var references = new AgentRunReferenceVault();
        var todoReference = references.TodoReference(room.Id, todo.Id, worker.Id);
        var deniedUpdate = await Assert.ThrowsAsync<AgentTeamException>(() => tools.ExecuteAsync(
            worker, workerMember, room, executorDelivery,
            new AgentToolCall("update", "todo_update", $$"""
                {"todo_ref":"{{todoReference}}","expected_revision":{{todo.Revision}},"status":"Completed"}
                """), CancellationToken.None, references, todo));
        Assert.Equal(AgentTeamError.PermissionDenied, deniedUpdate.Code);

        var workerCommunication = communication with { TargetAgentId = worker.Id };
        var deniedWorkerUpdate = await Assert.ThrowsAsync<AgentTeamException>(() =>
            tools.ExecuteAsync(worker, workerMember, room, workerCommunication,
                new AgentToolCall("manager-update", "todo_update", $$"""
                    {"todo_ref":"{{todoReference}}","expected_revision":{{todo.Revision}},"status":"Cancelled"}
                    """), CancellationToken.None, references));
        Assert.Equal(AgentTeamError.PermissionDenied, deniedWorkerUpdate.Code);

        var deniedCompletion = await Assert.ThrowsAsync<AgentTeamException>(() =>
            tools.ExecuteAsync(manager, managerMember, room, communication,
                new AgentToolCall("complete", "todo_complete", $$"""
                    {"todo_ref":"{{todoReference}}","expected_revision":{{todo.Revision}},"summary":"wrong lane"}
                    """), CancellationToken.None, references));
        Assert.Equal(AgentTeamError.PermissionDenied, deniedCompletion.Code);

        var completedResult = await tools.ExecuteAsync(worker, workerMember, room,
            executorDelivery, new AgentToolCall("complete", "todo_complete", $$"""
                {"todo_ref":"{{todoReference}}","expected_revision":{{todo.Revision}},"summary":"verified"}
                """), CancellationToken.None, references, todo);
        Assert.True(completedResult.EndsCycle);
        Assert.Equal(AgentTodoStatus.Completed,
            (await _store.GetTodoAsync("alice", todo.Id))!.Status);
        Assert.Equal(AgentTodoProgressKind.Completed,
            Assert.Single(await _store.ListTodoProgressAsync("alice", todo.Id),
                value => value.Kind == AgentTodoProgressKind.Completed).Kind);
    }

    [Fact]
    public async Task ManagerCommunicationCompletesWhileTodoExecutorIsStillRunning()
    {
        var manager = await _store.CreateAgentAsync("alice",
            Profile().Draft with { Name = "Manager" });
        var room = await _store.CreateRoomAsync("alice", "project-1",
            new("团队", "保持沟通响应"), manager.Id);
        await CompleteInitialMaintenanceAsync();
        _ = await _store.CreateTodoAsync("alice", new(room.Id, manager.Id, "长时间执行"));

        var executorStarted = new TaskCompletionSource<bool>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var releaseExecutor = new TaskCompletionSource<bool>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var managerReached = new TaskCompletionSource<bool>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var gateway = CreateGateway(async request =>
        {
            var body = await request.Content!.ReadAsStringAsync();
            if (body.Contains("execution_contract:", StringComparison.Ordinal))
            {
                executorStarted.TrySetResult(true);
                await releaseExecutor.Task.WaitAsync(TimeSpan.FromSeconds(5));
                var todoReference = Regex.Match(body, "todo_[a-f0-9]{32}").Value;
                Assert.NotEmpty(todoReference);
                return Json($$"""
                    {"status":"completed","output":[
                      {"type":"function_call","call_id":"complete","name":"todo_complete",
                       "arguments":"{\"todo_ref\":\"{{todoReference}}\",\"expected_revision\":2,\"summary\":\"done\"}"}
                    ]}
                    """);
            }

            managerReached.TrySetResult(true);
            return Json("""
                {"status":"completed","output":[
                  {"type":"message","content":[{"type":"output_text","text":"经理已响应。"}]}
                ]}
                """);
        });
        var scheduler = new AgentTeamScheduler(_store, gateway,
            new AgentTeamToolExecutor(_store, null!));

        var executorDrain = scheduler.DrainAsync("alice");
        await executorStarted.Task.WaitAsync(TimeSpan.FromSeconds(2));
        _ = await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Human, null, "执行期间请同步状态"));
        var communicationDrain = scheduler.DrainAsync("alice");

        await managerReached.Task.WaitAsync(TimeSpan.FromSeconds(2));
        Assert.False(executorDrain.IsCompleted);
        Assert.False(communicationDrain.IsCompleted);
        for (var attempt = 0; attempt < 50; attempt++)
        {
            var messages = await _store.ListMessagesAsync("alice", room.Id);
            if (messages.Any(value => value.Content == "经理已响应。")) break;
            await Task.Delay(20);
        }
        Assert.Contains(await _store.ListMessagesAsync("alice", room.Id),
            value => value.Content == "经理已响应。");

        releaseExecutor.TrySetResult(true);
        await Task.WhenAll(executorDrain, communicationDrain).WaitAsync(TimeSpan.FromSeconds(5));
    }

    private async Task CompleteInitialMaintenanceAsync()
    {
        var delivery = Assert.IsType<AgentDelivery>(
            await _store.ClaimNextDeliveryAsync("alice"));
        Assert.StartsWith("team-asset-maintenance:", delivery.DeduplicationKey,
            StringComparison.Ordinal);
        await _store.CompleteDeliveryAsync("alice", delivery.Id, null);
    }

    private static AgentTeamModelGateway CreateGateway(
        Func<HttpRequestMessage, Task<HttpResponseMessage>> provider)
    {
        var api = new ChatOSApiClient(new HttpClient(new AsyncHandler(request =>
            Task.FromResult(Json("""
                {"enabled":true,"model":"gpt-test","provider":"openai",
                 "api_key":"provider-secret","base_url":"https://provider.example/v1/chat/completions"}
                """))))
        {
            BaseAddress = new Uri("https://api.example/api/chatos/"),
        }, new EmptyTokenStore());
        return new AgentTeamModelGateway(api,
            new FixedHttpClientFactory(new HttpClient(new AsyncHandler(provider))));
    }

    private static AgentProfile Profile() => new(
        "agent-1", "alice",
        new("Agent", "说明", "完成任务", "model-1", "medium"),
        AgentProfileStatus.Active, 1, 1);

    private static HttpResponseMessage Json(string json, HttpStatusCode status = HttpStatusCode.OK) =>
        new(status) { Content = new StringContent(json, Encoding.UTF8, "application/json") };

    private sealed class AsyncHandler(
        Func<HttpRequestMessage, Task<HttpResponseMessage>> handler) : HttpMessageHandler
    {
        protected override Task<HttpResponseMessage> SendAsync(
            HttpRequestMessage request,
            CancellationToken cancellationToken) => handler(request);
    }

    private sealed class FixedHttpClientFactory(HttpClient client) : IHttpClientFactory
    {
        public HttpClient CreateClient(string name) => client;
    }

    private sealed class EmptyTokenStore : IAuthTokenStore
    {
        public ValueTask<string?> GetAccessTokenAsync(CancellationToken cancellationToken = default) =>
            ValueTask.FromResult<string?>(null);

        public ValueTask SetAccessTokenAsync(string token, CancellationToken cancellationToken = default) =>
            ValueTask.CompletedTask;

        public ValueTask ClearAsync(CancellationToken cancellationToken = default) =>
            ValueTask.CompletedTask;
    }
}
