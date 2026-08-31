using System.Net;
using ChatOS.Api.Conversation;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Tests;

public sealed class ConversationAttachmentServiceTests
{
    [Fact]
    public async Task UploadUsesPresignedTargetWithoutForwardingGatewayAuthorization()
    {
        var store = new MemoryTokenStore();
        store.Seed("gateway-token");
        var apiClient = ApiTestClient.Create(store, request =>
        {
            Assert.Equal("Bearer", request.Headers.Authorization?.Scheme);
            return StubHttpMessageHandler.Json("""
                {
                  "uploads":[{
                    "id":"attachment-1",
                    "name":"image.png",
                    "mimeType":"image/png",
                    "size":3,
                    "type":"image",
                    "storageProvider":"s3",
                    "bucket":"files",
                    "objectKey":"a/image.png",
                    "uploadUrl":"https://upload.example.test/a/image.png?signature=secret",
                    "uploadHeaders":{"Content-Type":"image/png","x-upload-token":"upload-only"},
                    "viewUrl":"https://files.example.test/a/image.png"
                  }]
                }
                """);
        });
        HttpRequestMessage? capturedUpload = null;
        byte[]? capturedBytes = null;
        var uploadClient = new HttpClient(new StubHttpMessageHandler(request =>
        {
            capturedUpload = request;
            capturedBytes = request.Content!.ReadAsByteArrayAsync().GetAwaiter().GetResult();
            return new HttpResponseMessage(HttpStatusCode.OK);
        }));
        var service = new ConversationAttachmentService(
            apiClient,
            new StubHttpClientFactory(uploadClient));
        var draft = ConversationAttachmentDraft.Create(
            "image.png",
            "image/png",
            ConversationAttachmentKind.Image,
            ConversationAttachmentOrigin.PastedImage,
            new byte[] { 1, 2, 3 });

        var reference = Assert.Single(await service.UploadAsync(new[] { draft }, "c1"));

        Assert.Null(capturedUpload?.Headers.Authorization);
        Assert.Equal("upload-only", capturedUpload?.Headers.GetValues("x-upload-token").Single());
        Assert.Equal("image/png", capturedUpload?.Content?.Headers.ContentType?.MediaType);
        Assert.Equal(new byte[] { 1, 2, 3 }, capturedBytes);
        Assert.Equal("attachment-1", reference.Id);
        Assert.Equal(ConversationAttachmentKind.Image, reference.Kind);
    }

    private sealed class StubHttpClientFactory : IHttpClientFactory
    {
        private readonly HttpClient _client;

        public StubHttpClientFactory(HttpClient client)
        {
            _client = client;
        }

        public HttpClient CreateClient(string name) => _client;
    }
}
