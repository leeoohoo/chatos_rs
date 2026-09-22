using System.Net;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;
using ChatOS.Api.Http;
using ChatOS.Connector.AgentTeams;
using ChatOS.Connector.Persistence;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class AgentTeamMultimodalInputTests : IAsyncLifetime
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(), "chatos-agent-multimodal-tests", Guid.NewGuid().ToString("N"));
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
    public async Task TriggerImagesAndPdfsUseResponsesMultimodalPartsAndOpaqueReferences()
    {
        var profile = await _store.CreateAgentAsync("alice", ProfileDraft("Manager"));
        var room = await _store.CreateRoomAsync("alice", "project-1",
            new("Team", "Inspect attachments"), profile.Id);
        await CompleteInitialMaintenanceAsync();
        var png = Attachment("durable-image-id", @"C:\private\screen.png", "image/png",
            AgentMessageAttachmentKind.Image, PngBytes());
        var pdf = Attachment("durable-pdf-id", "/private/design.pdf", "application/pdf",
            AgentMessageAttachmentKind.File, PdfBytes());
        var ignored = Attachment("durable-binary-id", "archive.bin", "application/octet-stream",
            AgentMessageAttachmentKind.File, [0, 1, 2, 3]);
        var post = await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Human, null, "Inspect these", Attachments: [png, pdf, ignored]));
        string? requestBody = null;
        var gateway = CreateGateway(async request =>
        {
            requestBody = await request.Content!.ReadAsStringAsync();
            return Json("""
                {"status":"completed","output":[
                  {"type":"message","content":[{"type":"output_text","text":"Reviewed."}]}
                ]}
                """);
        });

        await new AgentTeamScheduler(_store, gateway, new AgentTeamToolExecutor(_store, null!))
            .DrainAsync("alice");

        Assert.NotNull(requestBody);
        using var document = JsonDocument.Parse(requestBody!);
        var user = document.RootElement.GetProperty("input").EnumerateArray()
            .Single(value => value.GetProperty("role").GetString() == "user");
        var parts = user.GetProperty("content").EnumerateArray().ToArray();
        var image = Assert.Single(parts, value => value.GetProperty("type").GetString() == "input_image");
        var file = Assert.Single(parts, value => value.GetProperty("type").GetString() == "input_file");
        Assert.Equal($"data:image/png;base64,{Convert.ToBase64String(png.Data)}",
            image.GetProperty("image_url").GetString());
        Assert.Equal("design.pdf", file.GetProperty("filename").GetString());
        Assert.Equal($"data:application/pdf;base64,{Convert.ToBase64String(pdf.Data)}",
            file.GetProperty("file_data").GetString());
        Assert.Equal(2, parts.Count(value => value.GetProperty("type").GetString() == "input_text" &&
            value.GetProperty("text").GetString()!.StartsWith("[multimodal attachment_ref=",
                StringComparison.Ordinal)));
        Assert.DoesNotContain(png.Id, requestBody, StringComparison.Ordinal);
        Assert.DoesNotContain(pdf.Id, requestBody, StringComparison.Ordinal);
        Assert.DoesNotContain(post.Message.Id, requestBody, StringComparison.Ordinal);
        Assert.DoesNotContain(room.Id, requestBody, StringComparison.Ordinal);
        Assert.DoesNotContain(profile.Id, requestBody, StringComparison.Ordinal);
        Assert.DoesNotContain("C:\\private", requestBody, StringComparison.Ordinal);
        Assert.DoesNotContain("/private/", requestBody, StringComparison.Ordinal);
        Assert.DoesNotContain(Convert.ToBase64String(ignored.Data), requestBody, StringComparison.Ordinal);

        var references = new AgentRunReferenceVault();
        var member = Assert.Single(await _store.ListMembersAsync("alice", room.Id));
        var error = await Assert.ThrowsAsync<AgentTeamException>(() =>
            new AgentTeamToolExecutor(_store, null!).ExecuteAsync(profile, member, room,
                post.Deliveries.Single(), new AgentToolCall("read", "chat_read_attachment",
                    $$"""{"attachment_ref":"{{references.AttachmentReference(room.Id, post.Message.Id, pdf.Id)}}"}"""),
                CancellationToken.None, references));
        Assert.Equal(AgentTeamError.InvalidField, error.Code);
        Assert.Contains("not a supported text format", error.Message, StringComparison.Ordinal);
    }

    [Fact]
    public async Task SelectionEnforcesTypeSignatureCountAndBytePoliciesBeforeModelInput()
    {
        var profile = await _store.CreateAgentAsync("alice", ProfileDraft("Manager"));
        var room = await _store.CreateRoomAsync("alice", "project-1",
            new("Team", "Bound payloads"), profile.Id);
        await CompleteInitialMaintenanceAsync();
        var unsupported = Attachment("text-id", "notes.txt", "text/plain",
            AgentMessageAttachmentKind.File, "notes"u8.ToArray());
        var png = Attachment("png-id", "one.png", "image/png",
            AgentMessageAttachmentKind.Image, PngBytes());
        var pdf = Attachment("pdf-id", "two.pdf", "application/pdf",
            AgentMessageAttachmentKind.File, PdfBytes());
        var gif = Attachment("gif-id", "three.gif", "image/gif",
            AgentMessageAttachmentKind.Image, "GIF89aDATA"u8.ToArray());
        var spoofed = Attachment("spoof-id", "fake.png", "image/png",
            AgentMessageAttachmentKind.Image, "not-a-png!!"u8.ToArray());
        var message = (await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Agent, profile.Id, "sources",
                Attachments: [unsupported, png, pdf, gif, spoofed]))).Message;

        var itemBounded = await AgentTeamMultimodalInput.LoadAsync(_store, "alice", [message],
            new AgentRunReferenceVault(), CancellationToken.None,
            new AgentMultimodalPolicy(8, 10, 100));
        Assert.Equal(["pdf-id", "gif-id"], itemBounded.Select(value => value.AttachmentId));

        var totalBounded = await AgentTeamMultimodalInput.LoadAsync(_store, "alice", [message],
            new AgentRunReferenceVault(), CancellationToken.None,
            new AgentMultimodalPolicy(8, 20, 15));
        Assert.Equal("png-id", Assert.Single(totalBounded).AttachmentId);

        var countBounded = await AgentTeamMultimodalInput.LoadAsync(_store, "alice", [message],
            new AgentRunReferenceVault(), CancellationToken.None,
            new AgentMultimodalPolicy(2, 20, 100));
        Assert.Equal(["png-id", "pdf-id"], countBounded.Select(value => value.AttachmentId));
        Assert.All(countBounded, value => Assert.Matches("^attachment_[a-f0-9]{32}$", value.Reference));
        Assert.DoesNotContain(countBounded, value => value.AttachmentId == spoofed.Id);
    }

    [Fact]
    public async Task TodoExecutorReceivesMultimodalPayloadOnlyFromFrozenSourceContext()
    {
        var profile = await _store.CreateAgentAsync("alice", ProfileDraft("Worker"));
        var room = await _store.CreateRoomAsync("alice", "project-1",
            new("Team", "Execute source contract"), profile.Id);
        await CompleteInitialMaintenanceAsync();
        var sourceAttachment = Attachment("source-image-durable-id", "source.png", "image/png",
            AgentMessageAttachmentKind.Image, PngBytes());
        var source = await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Agent, profile.Id, "SOURCE_CONTEXT",
                Attachments: [sourceAttachment]));
        _ = await _store.CreateTodoAsync("alice", new(room.Id, profile.Id, "Inspect source",
            SourceMessageId: source.Message.Id,
            ExecutionContract: new AgentTodoExecutionContract(
                "VERIFY_IMAGE", "source only", ["result"], ["checked"])));
        string? requestBody = null;
        var gateway = CreateGateway(async request =>
        {
            requestBody = await request.Content!.ReadAsStringAsync();
            return Json("""
                {"status":"completed","output":[
                  {"type":"message","content":[{"type":"output_text","text":"Checked."}]}
                ]}
                """);
        });

        await new AgentTeamScheduler(_store, gateway, new AgentTeamToolExecutor(_store, null!))
            .DrainAsync("alice");

        Assert.NotNull(requestBody);
        Assert.Contains("VERIFY_IMAGE", requestBody, StringComparison.Ordinal);
        Assert.Contains("SOURCE_CONTEXT", requestBody, StringComparison.Ordinal);
        Assert.Contains("\"type\":\"input_image\"", requestBody, StringComparison.Ordinal);
        Assert.Contains(Convert.ToBase64String(sourceAttachment.Data), requestBody,
            StringComparison.Ordinal);
        Assert.DoesNotContain(sourceAttachment.Id, requestBody, StringComparison.Ordinal);
        Assert.DoesNotContain(source.Message.Id, requestBody, StringComparison.Ordinal);
        Assert.DoesNotContain(room.Id, requestBody, StringComparison.Ordinal);
        Assert.DoesNotContain(profile.Id, requestBody, StringComparison.Ordinal);
    }

    private async Task CompleteInitialMaintenanceAsync()
    {
        var delivery = Assert.IsType<AgentDelivery>(await _store.ClaimNextDeliveryAsync("alice"));
        await _store.CompleteDeliveryAsync("alice", delivery.Id, null);
    }

    private static AgentMessageAttachment Attachment(
        string id,
        string name,
        string mimeType,
        AgentMessageAttachmentKind kind,
        byte[] data) => new(id, name, mimeType, kind, data.LongLength, data);

    private static byte[] PngBytes() =>
        [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x01, 0x02, 0x03];

    private static byte[] PdfBytes() => "%PDF-1.7\n"u8.ToArray();

    private static AgentProfileDraft ProfileDraft(string name) =>
        new(name, "Description", "Role", "model-1", "medium");

    private static AgentTeamModelGateway CreateGateway(
        Func<HttpRequestMessage, Task<HttpResponseMessage>> provider)
    {
        var api = new ChatOSApiClient(new HttpClient(new AsyncHandler(_ =>
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
