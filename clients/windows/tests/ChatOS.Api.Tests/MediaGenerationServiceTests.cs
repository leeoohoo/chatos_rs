using System.Net;
using System.Text;
using System.Text.Json;
using ChatOS.Api.Http;
using ChatOS.Api.Media;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Tests;

public sealed class MediaGenerationServiceTests
{
    [Fact]
    public async Task FetchModelsFiltersUnavailableModelsAndOrdersImagesFirst()
    {
        var store = TokenStore();
        var api = ApiTestClient.Create(store, _ => StubHttpMessageHandler.Json("""
            [
              {"id":"video","name":"Sora Video","provider":"openai","model":"sora-2","enabled":true,"has_api_key":true},
              {"id":"disabled","name":"Disabled","model":"image","enabled":false,"has_api_key":true},
              {"id":"missing-key","name":"No Key","model":"image","enabled":true,"has_api_key":false},
              {"id":"image-b","name":"Beta","provider":"openai","model":"gpt-image-1","enabled":true,"has_api_key":true},
              {"id":"image-a","name":"Alpha","provider":"openai","model":"gpt-image-1","enabled":true,"has_api_key":true}
            ]
            """));
        var service = new MediaGenerationService(api, EmptyProviderFactory(), store);

        var models = await service.FetchModelsAsync();

        Assert.Equal(new[] { "image-a", "image-b", "video" }, models.Select(model => model.Id));
    }

    [Fact]
    public async Task GenerateImagePostsJsonToNormalizedGenerationsEndpoint()
    {
        var store = TokenStore();
        var api = RuntimeApi(store, "https://provider.example.test/v1/chat/completions");
        Uri? requestUri = null;
        string? authorization = null;
        string? body = null;
        var provider = ProviderFactory(async request =>
        {
            requestUri = request.RequestUri;
            authorization = request.Headers.Authorization?.ToString();
            body = await request.Content!.ReadAsStringAsync();
            return Json("""{"id":"result-1","model":"image-v2","data":[{"id":"image-1","b64_json":"AQID","mime_type":"image/png"}]}""");
        });
        var service = new MediaGenerationService(api, provider, store);

        var result = await service.GenerateImageAsync(Request());

        Assert.Equal("https://provider.example.test/v1/images/generations", requestUri?.AbsoluteUri);
        Assert.Equal("Bearer provider-secret", authorization);
        using var json = JsonDocument.Parse(body!);
        Assert.Equal("image-v1", json.RootElement.GetProperty("model").GetString());
        Assert.Equal("draw a fox", json.RootElement.GetProperty("prompt").GetString());
        Assert.Equal(2, json.RootElement.GetProperty("n").GetInt32());
        Assert.Equal("1024x1024", json.RootElement.GetProperty("size").GetString());
        Assert.Equal("result-1", result.Id);
        Assert.Equal("AQID", Assert.Single(result.Images).Base64Data);
    }

    [Fact]
    public async Task GenerateImagePostsMultipartForReferenceImages()
    {
        var store = TokenStore();
        var api = RuntimeApi(store, "https://provider.example.test/v1/images/generations");
        string? contentType = null;
        string? body = null;
        var provider = ProviderFactory(async request =>
        {
            contentType = request.Content?.Headers.ContentType?.MediaType;
            body = await request.Content!.ReadAsStringAsync();
            return Json("""{"data":[{"url":"https://cdn.example.test/output.png"}]}""");
        });
        var service = new MediaGenerationService(api, provider, store);
        var input = new ImageGenerationInput("face.png", "image/png", Convert.ToBase64String([1, 2, 3]));

        var result = await service.GenerateImageAsync(Request([input]));

        Assert.Equal("multipart/form-data", contentType);
        Assert.Contains("name=\"image\"; filename=\"face.png\"", body!);
        Assert.Contains("name=\"prompt\"", body!);
        Assert.Equal("https://cdn.example.test/output.png", Assert.Single(result.Images).Url?.AbsoluteUri);
    }

    [Fact]
    public async Task GenerateImageRejectsInvalidProviderBase64()
    {
        var store = TokenStore();
        var service = new MediaGenerationService(
            RuntimeApi(store),
            ProviderFactory(_ => Task.FromResult(Json("""{"data":[{"b64_json":"not-base64"}]}"""))),
            store);

        var error = await Assert.ThrowsAsync<ChatOSApiException>(() =>
            service.GenerateImageAsync(Request()));

        Assert.Contains("invalid image data", error.Message, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public async Task GenerateImageSurfacesProviderErrorDetail()
    {
        var store = TokenStore();
        var service = new MediaGenerationService(
            RuntimeApi(store),
            ProviderFactory(_ => Task.FromResult(Json(
                """{"error":{"message":"quota exhausted"}}""",
                HttpStatusCode.TooManyRequests))),
            store);

        var error = await Assert.ThrowsAsync<ChatOSApiException>(() =>
            service.GenerateImageAsync(Request()));

        Assert.Equal(HttpStatusCode.TooManyRequests, error.StatusCode);
        Assert.Contains("quota exhausted", error.Message);
    }

    [Fact]
    public async Task GenerateImageRejectsResultAfterAccountChanges()
    {
        var store = TokenStore();
        var service = new MediaGenerationService(
            RuntimeApi(store),
            ProviderFactory(_ =>
            {
                store.Seed("different-account-token");
                return Task.FromResult(Json("""{"data":[{"b64_json":"AQID"}]}"""));
            }),
            store);

        var error = await Assert.ThrowsAsync<ChatOSApiException>(() =>
            service.GenerateImageAsync(Request()));

        Assert.Contains("account changed", error.Message, StringComparison.OrdinalIgnoreCase);
    }

    private static ImageGenerationRequest Request(IReadOnlyList<ImageGenerationInput>? references = null) =>
        new("model-config", "draw a fox", "1024x1024", 2, references ?? []);

    private static MemoryTokenStore TokenStore()
    {
        var store = new MemoryTokenStore();
        store.Seed("gateway-token");
        return store;
    }

    private static ChatOSApiClient RuntimeApi(
        MemoryTokenStore store,
        string baseUrl = "https://provider.example.test/v1") =>
        ApiTestClient.Create(store, request =>
        {
            Assert.Contains("ai-model-configs/model-config", request.RequestUri!.AbsoluteUri);
            Assert.Contains("include_secret=true", request.RequestUri.Query);
            return StubHttpMessageHandler.Json($$"""
                {"model":"image-v1","api_key":"provider-secret","base_url":"{{baseUrl}}","enabled":true}
                """);
        });

    private static IHttpClientFactory EmptyProviderFactory() =>
        ProviderFactory(_ => Task.FromResult(Json("{}")));

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
            Assert.Equal(MediaGenerationService.ProviderClientName, name);
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
