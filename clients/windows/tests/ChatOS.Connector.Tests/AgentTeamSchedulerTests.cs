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
    public async Task TodoCompletionDoesNotCancelItsActiveDelivery()
    {
        var profile = await _store.CreateAgentAsync("alice", Profile().Draft);
        var room = await _store.CreateRoomAsync("alice", "project-1",
            new("团队", "完成工作"), profile.Id);
        await CompleteInitialMaintenanceAsync();
        var todo = await _store.CreateTodoAsync("alice",
            new(room.Id, profile.Id, "实现功能"));
        var gateway = CreateGateway(async request =>
        {
            var body = await request.Content!.ReadAsStringAsync();
            var todoReference = Regex.Match(body, "todo_[a-f0-9]{32}").Value;
            Assert.NotEmpty(todoReference);
            return Json($$"""
                {"status":"completed","output":[
                  {"type":"function_call","call_id":"call-complete","name":"todo_update",
                   "arguments":"{\"todo_ref\":\"{{todoReference}}\",\"expected_revision\":2,\"status\":\"Completed\",\"result\":\"完成\"}"}
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
