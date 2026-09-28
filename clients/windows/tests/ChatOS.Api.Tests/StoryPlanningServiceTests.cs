using System.Net;
using System.Text;
using System.Text.Json;
using ChatOS.Api.Http;
using ChatOS.Api.Media;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Tests;

public sealed class StoryPlanningServiceTests
{
    [Fact]
    public async Task PlanPostsStructuredResponsesRequestAndDecodesSegments()
    {
        var store = TokenStore();
        string? body = null;
        Uri? uri = null;
        var plan = """
            {"summary":"A short journey","resources":[{"id":"train","kind":"prop","name":"Train","description":"A passenger train","image_prompt":"A blue passenger train"}],"segments":[{"kind":"story","title":"Departure","narrative":"The train leaves.","image_prompt":"A train at dawn","video_prompt":"The train pulls away slowly","seconds":5,"resource_ids":["train"]}]}
            """;
        var provider = ProviderFactory(async request =>
        {
            uri = request.RequestUri;
            body = await request.Content!.ReadAsStringAsync();
            var envelope = JsonSerializer.Serialize(new
            {
                output = new[]
                {
                    new
                    {
                        type = "message",
                        content = new[] { new { type = "output_text", text = plan } },
                    },
                },
            });
            return Json(envelope);
        });
        var service = new StoryPlanningService(RuntimeApi(store), provider, store);

        var result = await service.PlanAsync(Request());

        Assert.Equal("https://provider.example.test/v1/responses", uri?.AbsoluteUri);
        using var requestJson = JsonDocument.Parse(body!);
        Assert.Equal("text-v1", requestJson.RootElement.GetProperty("model").GetString());
        Assert.Equal("json_schema", requestJson.RootElement.GetProperty("text").GetProperty("format").GetProperty("type").GetString());
        var segmentKind = requestJson.RootElement.GetProperty("text").GetProperty("format")
            .GetProperty("schema").GetProperty("properties").GetProperty("segments")
            .GetProperty("items").GetProperty("properties").GetProperty("kind");
        Assert.Equal("transition", segmentKind.GetProperty("enum")[1].GetString());
        Assert.Equal("A short journey", result.Summary);
        Assert.Equal("train", Assert.Single(result.Resources).Id);
        var segment = Assert.Single(result.Segments);
        Assert.Equal("story", segment.Kind);
        Assert.Equal("Departure", segment.Title);
        Assert.Equal(5, segment.Seconds);
        Assert.Equal("train", Assert.Single(segment.ResourceIds));
    }

    [Fact]
    public async Task PlanRejectsInvalidProviderDurations()
    {
        var store = TokenStore();
        var text = """
            {"summary":"Bad","resources":[],"segments":[{"kind":"story","title":"Shot","narrative":"Story","image_prompt":"Image","video_prompt":"Video","seconds":30,"resource_ids":[]}]}
            """;
        var provider = ProviderFactory(_ => Task.FromResult(Json(
            JsonSerializer.Serialize(new { output_text = text }))));
        var service = new StoryPlanningService(RuntimeApi(store), provider, store);

        var error = await Assert.ThrowsAsync<ChatOSApiException>(() => service.PlanAsync(Request()));

        Assert.Contains("invalid plan", error.Message, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public async Task PlanRejectsResultAfterAccountChanges()
    {
        var store = TokenStore();
        var provider = ProviderFactory(_ =>
        {
            store.Seed("different-token");
            return Task.FromResult(Json("""
                {"output_text":"{\"summary\":\"Summary\",\"resources\":[],\"segments\":[{\"kind\":\"story\",\"title\":\"Shot\",\"narrative\":\"Story\",\"image_prompt\":\"Image\",\"video_prompt\":\"Video\",\"seconds\":4,\"resource_ids\":[]}] }"}
                """));
        });
        var service = new StoryPlanningService(RuntimeApi(store), provider, store);

        var error = await Assert.ThrowsAsync<ChatOSApiException>(() => service.PlanAsync(Request()));

        Assert.Contains("account changed", error.Message, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public async Task OptimizePostsReviewableStructuredSuggestion()
    {
        var store = TokenStore();
        string? body = null;
        var provider = ProviderFactory(async request =>
        {
            body = await request.Content!.ReadAsStringAsync();
            return Json(JsonSerializer.Serialize(new
            {
                output_text = "{\"optimized_text\":\"A tighter story.\",\"rationale\":\"Improved pacing.\"}",
            }));
        });
        var service = new StoryPlanningService(RuntimeApi(store), provider, store);

        var result = await service.OptimizeAsync(new StoryOptimizationRequest(
            "model-config", "Train story", "A journey", "The train leaves.",
            "cinematic natural light", StoryOptimizationTarget.Source));

        Assert.Equal("A tighter story.", result.OptimizedText);
        Assert.Equal("Improved pacing.", result.Rationale);
        using var json = JsonDocument.Parse(body!);
        var root = json.RootElement;
        Assert.Equal("story_optimization", root.GetProperty("text").GetProperty("format").GetProperty("name").GetString());
        Assert.Contains("不改变人物、事件、因果与结局",
            root.GetProperty("input")[0].GetProperty("content").GetString());
        Assert.Equal(80_000, root.GetProperty("text").GetProperty("format").GetProperty("schema")
            .GetProperty("properties").GetProperty("optimized_text").GetProperty("maxLength").GetInt32());
    }

    [Fact]
    public async Task OptimizeRejectsOversizedStyleSuggestion()
    {
        var store = TokenStore();
        var output = JsonSerializer.Serialize(new
        {
            optimized_text = new string('x', 2_001),
            rationale = "Reason",
        });
        var provider = ProviderFactory(_ => Task.FromResult(Json(
            JsonSerializer.Serialize(new { output_text = output }))));
        var service = new StoryPlanningService(RuntimeApi(store), provider, store);

        var error = await Assert.ThrowsAsync<ChatOSApiException>(() => service.OptimizeAsync(
            new StoryOptimizationRequest(
                "model-config", "Train story", "A journey", "The train leaves.",
                "cinematic natural light", StoryOptimizationTarget.VisualStyle)));

        Assert.Contains("invalid optimization", error.Message, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public async Task RefineSegmentPostsContinuityAwareStructuredRequest()
    {
        var store = TokenStore();
        string? body = null;
        var refinement = """
            {"image_prompt":"A train enters a tunnel","video_prompt":"Dissolve from station to tunnel","continuity_in":"Train waits at station","continuity_out":"Train emerges at night","shot_plan":"0-1s hold; 1-3s dissolve","rationale":"Connects time and place without adding plot."}
            """;
        var provider = ProviderFactory(async request =>
        {
            body = await request.Content!.ReadAsStringAsync();
            return Json(JsonSerializer.Serialize(new { output_text = refinement }));
        });
        var service = new StoryPlanningService(RuntimeApi(store), provider, store);

        var result = await service.RefineSegmentAsync(new StorySegmentRefinementRequest(
            "model-config", "Train story", "A journey", "cinematic natural light", "16:9",
            "segment-2", "transition", "Nightfall", "Time passes", 3, "Train at station", "Dissolve",
            "Previous shot ends at the station", "scene station: fixed platform layout"));

        Assert.Equal("0-1s hold; 1-3s dissolve", result.ShotPlan);
        Assert.Equal("Train emerges at night", result.ContinuityOut);
        using var json = JsonDocument.Parse(body!);
        var root = json.RootElement;
        Assert.Equal("story_segment_refinement",
            root.GetProperty("text").GetProperty("format").GetProperty("name").GetString());
        Assert.Contains("转场段只连接前后画面状态",
            root.GetProperty("input")[0].GetProperty("content").GetString());
        Assert.Contains("Previous shot ends at the station",
            root.GetProperty("input")[1].GetProperty("content").GetString());
    }

    private static StoryPlanningRequest Request() => new(
        "model-config", "Train story", "A journey", "The train leaves the station.",
        "cinematic natural light", "16:9");

    private static MemoryTokenStore TokenStore()
    {
        var store = new MemoryTokenStore();
        store.Seed("gateway-token");
        return store;
    }

    private static ChatOSApiClient RuntimeApi(MemoryTokenStore store) =>
        ApiTestClient.Create(store, request =>
        {
            Assert.Contains("ai-model-configs/model-config", request.RequestUri!.AbsoluteUri);
            return StubHttpMessageHandler.Json("""
                {"model":"text-v1","api_key":"provider-secret","base_url":"https://provider.example.test/v1/chat/completions","enabled":true}
                """);
        });

    private static IHttpClientFactory ProviderFactory(
        Func<HttpRequestMessage, Task<HttpResponseMessage>> response) =>
        new FixedHttpClientFactory(new HttpClient(new AsyncHttpMessageHandler(response)));

    private static HttpResponseMessage Json(
        string body,
        HttpStatusCode status = HttpStatusCode.OK) =>
        new(status) { Content = new StringContent(body, Encoding.UTF8, "application/json") };

    private sealed class FixedHttpClientFactory(HttpClient client) : IHttpClientFactory
    {
        public HttpClient CreateClient(string name)
        {
            Assert.Equal(StoryPlanningService.ProviderClientName, name);
            return client;
        }
    }

    private sealed class AsyncHttpMessageHandler(
        Func<HttpRequestMessage, Task<HttpResponseMessage>> response) : HttpMessageHandler
    {
        protected override Task<HttpResponseMessage> SendAsync(
            HttpRequestMessage request,
            CancellationToken cancellationToken) => response(request);
    }
}
