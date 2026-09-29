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
        string? imageField = null;
        string? imageFileName = null;
        byte[]? imageBytes = null;
        var provider = ProviderFactory(async request =>
        {
            contentType = request.Content?.Headers.ContentType?.MediaType;
            var multipart = Assert.IsType<MultipartFormDataContent>(request.Content);
            var imagePart = Assert.Single(multipart, part =>
                part.Headers.ContentDisposition?.Name?.Trim('"') == "image");
            imageField = imagePart.Headers.ContentDisposition?.Name?.Trim('"');
            imageFileName = imagePart.Headers.ContentDisposition?.FileName?.Trim('"');
            imageBytes = await imagePart.ReadAsByteArrayAsync();
            body = await request.Content!.ReadAsStringAsync();
            return Json("""{"data":[{"url":"https://cdn.example.test/output.png"}]}""");
        });
        var service = new MediaGenerationService(api, provider, store);
        var input = new ImageGenerationInput("face.png", "image/png", Convert.ToBase64String([1, 2, 3]));

        var result = await service.GenerateImageAsync(Request([input]));

        Assert.Equal("multipart/form-data", contentType);
        Assert.Equal("image", imageField);
        Assert.Equal("face.png", imageFileName);
        Assert.Equal(new byte[] { 1, 2, 3 }, imageBytes);
        Assert.Contains("draw a fox", body!);
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

    [Fact]
    public async Task GenerateVideoCreatesPollsAndDownloadsUnifiedVideoJob()
    {
        var store = TokenStore();
        var api = RuntimeApi(store, "https://provider.example.test/v1", "minimax-h3");
        var statusQueries = 0;
        string? createBody = null;
        var provider = ProviderFactory(async request =>
        {
            Assert.Equal("Bearer provider-secret", request.Headers.Authorization?.ToString());
            if (request.Method == HttpMethod.Post)
            {
                createBody = await request.Content!.ReadAsStringAsync();
                return Json("""{"task_id":"job/1","status":"processing","progress":25}""");
            }
            if (request.RequestUri!.AbsolutePath.EndsWith("/content", StringComparison.Ordinal))
            {
                return new HttpResponseMessage(HttpStatusCode.OK)
                {
                    Content = new ByteArrayContent([4, 5, 6]),
                }.WithContentType("video/mp4");
            }
            statusQueries++;
            return Json("""{"id":"job/1","status":"completed","progress":100,"model":"minimax-h3"}""");
        });
        var service = new MediaGenerationService(api, provider, store, TimeSpan.Zero, 5);
        var updates = new List<VideoGenerationProgress>();

        var request = VideoRequest() with
        {
            ReferenceAudio = new VideoGenerationInputAudio("guide.mp3", "audio/mpeg", "AQID"),
        };
        var result = await service.GenerateVideoAsync(
            request,
            new InlineProgress<VideoGenerationProgress>(updates.Add));

        Assert.Equal(1, statusQueries);
        Assert.Equal(new byte[] { 4, 5, 6 }, result.VideoData);
        Assert.Equal("video/mp4", result.MimeType);
        Assert.Equal(new[] { "processing", "completed", "downloading" }, updates.Select(value => value.Status));
        using var json = JsonDocument.Parse(createBody!);
        Assert.Equal(4, json.RootElement.GetProperty("duration").GetInt32());
        Assert.Equal("768p", json.RootElement.GetProperty("size").GetString());
        Assert.Equal("16:9", json.RootElement.GetProperty("metadata").GetProperty("ratio").GetString());
        Assert.Equal(
            "data:audio/mpeg;base64,AQID",
            json.RootElement.GetProperty("metadata").GetProperty("audio_url").GetString());
    }

    [Fact]
    public async Task ResumeVideoUsesSignedMetadataUrlWithoutProviderAuthorization()
    {
        var store = TokenStore();
        var api = RuntimeApi(store, "https://provider.example.test/v1", "minimax-h3");
        var requests = new List<(Uri Uri, string? Authorization)>();
        var provider = ProviderFactory(request =>
        {
            requests.Add((request.RequestUri!, request.Headers.Authorization?.ToString()));
            if (request.RequestUri!.Host == "cdn.example.test")
            {
                return Task.FromResult(new HttpResponseMessage(HttpStatusCode.OK)
                {
                    Content = new ByteArrayContent([7, 8, 9]),
                }.WithContentType("video/mp4"));
            }
            return Task.FromResult(Json("""
                {"id":"job-2","status":"completed","metadata":{"url":"https://cdn.example.test/signed/video.mp4?token=secret"}}
                """));
        });
        var service = new MediaGenerationService(api, provider, store, TimeSpan.Zero, 5);

        var result = await service.ResumeVideoAsync(VideoRequest(), "job-2");

        Assert.Equal(new byte[] { 7, 8, 9 }, result.VideoData);
        Assert.Equal("Bearer provider-secret", requests[0].Authorization);
        Assert.Null(requests[1].Authorization);
    }

    [Fact]
    public async Task GenerateVideoSendsReferenceVideoForEditing()
    {
        var store = TokenStore();
        var api = RuntimeApi(store, "https://provider.example.test/v1", "minimax-h3");
        string? createBody = null;
        var provider = ProviderFactory(async request =>
        {
            if (request.Method == HttpMethod.Post)
            {
                createBody = await request.Content!.ReadAsStringAsync();
                return Json("""{"id":"job-edit","status":"completed"}""");
            }
            return new HttpResponseMessage(HttpStatusCode.OK)
            {
                Content = new ByteArrayContent([1, 2, 3]),
            }.WithContentType("video/mp4");
        });
        var service = new MediaGenerationService(api, provider, store, TimeSpan.Zero, 5);
        var request = VideoRequest() with
        {
            ReferenceVideo = new VideoGenerationInputVideo("source.mp4", "video/mp4", "AQID"),
            ReferencePurpose = VideoGenerationReferencePurpose.Edit,
        };

        await service.GenerateVideoAsync(request);

        using var json = JsonDocument.Parse(createBody!);
        var metadata = json.RootElement.GetProperty("metadata");
        Assert.Equal("adaptive", metadata.GetProperty("ratio").GetString());
        Assert.Equal("data:video/mp4;base64,AQID", metadata.GetProperty("video_url").GetString());
        Assert.False(metadata.TryGetProperty("first_frame_image", out _));
    }

    [Fact]
    public async Task GenerateVideoRejectsUnsupportedLastFrameBeforeSubmitting()
    {
        var store = TokenStore();
        var api = RuntimeApi(store, "https://provider.example.test/v1", "sora-2");
        var service = new MediaGenerationService(api, EmptyProviderFactory(), store, TimeSpan.Zero, 5);
        var frame = new ImageGenerationInput("frame.png", "image/png", "AQID");
        var request = VideoRequest() with
        {
            Size = "1280x720",
            FirstFrame = frame,
            LastFrame = frame,
        };

        var error = await Assert.ThrowsAsync<ArgumentException>(() =>
            service.GenerateVideoAsync(request));

        Assert.Contains("last frame", error.Message, StringComparison.OrdinalIgnoreCase);
    }

    private static ImageGenerationRequest Request(IReadOnlyList<ImageGenerationInput>? references = null) =>
        new("model-config", "draw a fox", "1024x1024", 2, references ?? []);

    private static VideoGenerationRequest VideoRequest() =>
        new("model-config", "a fox running through snow", "768P", 4, null, null, null, "16:9");

    private static MemoryTokenStore TokenStore()
    {
        var store = new MemoryTokenStore();
        store.Seed("gateway-token");
        return store;
    }

    private static ChatOSApiClient RuntimeApi(
        MemoryTokenStore store,
        string baseUrl = "https://provider.example.test/v1",
        string model = "image-v1") =>
        ApiTestClient.Create(store, request =>
        {
            Assert.Contains("ai-model-configs/model-config", request.RequestUri!.AbsoluteUri);
            Assert.Contains("include_secret=true", request.RequestUri.Query);
            return StubHttpMessageHandler.Json($$"""
                {"model":"{{model}}","api_key":"provider-secret","base_url":"{{baseUrl}}","enabled":true}
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

    private sealed class InlineProgress<T>(Action<T> report) : IProgress<T>
    {
        public void Report(T value) => report(value);
    }
}

internal static class HttpResponseMessageTestExtensions
{
    public static HttpResponseMessage WithContentType(
        this HttpResponseMessage response,
        string mediaType)
    {
        response.Content.Headers.ContentType = new System.Net.Http.Headers.MediaTypeHeaderValue(mediaType);
        return response;
    }
}
