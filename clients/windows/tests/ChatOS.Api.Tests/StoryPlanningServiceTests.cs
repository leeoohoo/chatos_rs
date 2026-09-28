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
            {"summary":"A short journey","segments":[{"title":"Departure","narrative":"The train leaves.","image_prompt":"A train at dawn","video_prompt":"The train pulls away slowly","seconds":5}]}
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
        Assert.Equal("A short journey", result.Summary);
        var segment = Assert.Single(result.Segments);
        Assert.Equal("Departure", segment.Title);
        Assert.Equal(5, segment.Seconds);
    }

    [Fact]
    public async Task PlanRejectsInvalidProviderDurations()
    {
        var store = TokenStore();
        var text = """
            {"summary":"Bad","segments":[{"title":"Shot","narrative":"Story","image_prompt":"Image","video_prompt":"Video","seconds":30}]}
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
                {"output_text":"{\"summary\":\"Summary\",\"segments\":[{\"title\":\"Shot\",\"narrative\":\"Story\",\"image_prompt\":\"Image\",\"video_prompt\":\"Video\",\"seconds\":4}]}"}
                """));
        });
        var service = new StoryPlanningService(RuntimeApi(store), provider, store);

        var error = await Assert.ThrowsAsync<ChatOSApiException>(() => service.PlanAsync(Request()));

        Assert.Contains("account changed", error.Message, StringComparison.OrdinalIgnoreCase);
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
