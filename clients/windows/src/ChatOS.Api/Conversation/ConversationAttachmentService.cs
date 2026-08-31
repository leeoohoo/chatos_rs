using System.Net.Http.Headers;
using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Conversation;

public sealed class ConversationAttachmentService : IConversationAttachmentService
{
    internal const string UploadClientName = "ChatOS.AttachmentUpload";
    private readonly ChatOSApiClient _client;
    private readonly IHttpClientFactory _httpClientFactory;

    public ConversationAttachmentService(
        ChatOSApiClient client,
        IHttpClientFactory httpClientFactory)
    {
        _client = client;
        _httpClientFactory = httpClientFactory;
    }

    public async Task<IReadOnlyList<ConversationAttachmentReference>> UploadAsync(
        IReadOnlyList<ConversationAttachmentDraft> attachments,
        string conversationId,
        CancellationToken cancellationToken = default)
    {
        if (attachments.Count == 0)
        {
            return Array.Empty<ConversationAttachmentReference>();
        }

        var response = await _client.PostAsync<AttachmentUploadsResponseDto>(
            "attachments/uploads",
            new AttachmentUploadsRequestDto(
                conversationId,
                attachments.Select(AttachmentUploadItemDto.FromDomain).ToArray()),
            cancellationToken).ConfigureAwait(false);
        if (response.Uploads.Count != attachments.Count)
        {
            throw new ChatOSApiException("The gateway returned an unexpected number of attachment upload targets.");
        }

        var uploadClient = _httpClientFactory.CreateClient(UploadClientName);
        for (var index = 0; index < attachments.Count; index++)
        {
            var draft = attachments[index];
            var target = response.Uploads[index];
            using var request = new HttpRequestMessage(HttpMethod.Put, target.UploadUrl)
            {
                Content = new ByteArrayContent(draft.Data),
            };
            request.Content.Headers.ContentType = MediaTypeHeaderValue.Parse(draft.MimeType);
            foreach (var header in target.UploadHeaders ?? new Dictionary<string, string>())
            {
                if (header.Key.Equals("Host", StringComparison.OrdinalIgnoreCase) ||
                    header.Key.Equals("Content-Length", StringComparison.OrdinalIgnoreCase))
                {
                    continue;
                }

                if (header.Key.Equals("Content-Type", StringComparison.OrdinalIgnoreCase))
                {
                    request.Content.Headers.ContentType = MediaTypeHeaderValue.Parse(header.Value);
                    continue;
                }

                if (!request.Headers.TryAddWithoutValidation(header.Key, header.Value))
                {
                    request.Content.Headers.TryAddWithoutValidation(header.Key, header.Value);
                }
            }

            using var uploadResponse = await uploadClient.SendAsync(request, cancellationToken)
                .ConfigureAwait(false);
            if (!uploadResponse.IsSuccessStatusCode)
            {
                throw new ChatOSApiException(
                    $"Attachment '{draft.Name}' could not be uploaded.",
                    uploadResponse.StatusCode);
            }
        }

        return response.Uploads.Select(static upload => upload.ToDomain()).ToArray();
    }
}

internal sealed record AttachmentUploadsRequestDto(
    [property: JsonPropertyName("conversation_id")] string ConversationId,
    [property: JsonPropertyName("attachments")] IReadOnlyList<AttachmentUploadItemDto> Attachments);

internal sealed record AttachmentUploadItemDto(
    [property: JsonPropertyName("name")] string Name,
    [property: JsonPropertyName("mimeType")] string MimeType,
    [property: JsonPropertyName("size")] int Size,
    [property: JsonPropertyName("type")] string Kind)
{
    public static AttachmentUploadItemDto FromDomain(ConversationAttachmentDraft draft) => new(
        draft.Name,
        draft.MimeType,
        draft.Size,
        draft.Kind.ToApiValue());
}

internal sealed record AttachmentUploadsResponseDto(
    [property: JsonPropertyName("uploads")] IReadOnlyList<AttachmentUploadTargetDto> Uploads);

internal sealed record AttachmentUploadTargetDto
{
    [JsonPropertyName("id")]
    public string? Id { get; init; }

    [JsonPropertyName("name")]
    public required string Name { get; init; }

    [JsonPropertyName("mimeType")]
    public required string MimeType { get; init; }

    [JsonPropertyName("size")]
    public int Size { get; init; }

    [JsonPropertyName("type")]
    public required string Kind { get; init; }

    [JsonPropertyName("storageProvider")]
    public string? StorageProvider { get; init; }

    [JsonPropertyName("bucket")]
    public string? Bucket { get; init; }

    [JsonPropertyName("objectKey")]
    public string? ObjectKey { get; init; }

    [JsonPropertyName("uploadUrl")]
    public required string UploadUrl { get; init; }

    [JsonPropertyName("uploadHeaders")]
    public IReadOnlyDictionary<string, string>? UploadHeaders { get; init; }

    [JsonPropertyName("url")]
    public string? Url { get; init; }

    [JsonPropertyName("viewUrl")]
    public string? ViewUrl { get; init; }

    public ConversationAttachmentReference ToDomain() => new(
        Id.TrimmedOrNull() ?? Guid.NewGuid().ToString("N"),
        Name,
        MimeType,
        Size,
        Kind.ToAttachmentKind(),
        StorageProvider,
        Bucket,
        ObjectKey,
        Url,
        ViewUrl);
}

internal static class AttachmentKindExtensions
{
    public static string ToApiValue(this ConversationAttachmentKind kind) => kind switch
    {
        ConversationAttachmentKind.Image => "image",
        ConversationAttachmentKind.File => "file",
        ConversationAttachmentKind.Audio => "audio",
        _ => throw new ArgumentOutOfRangeException(nameof(kind), kind, null),
    };

    public static ConversationAttachmentKind ToAttachmentKind(this string kind) =>
        kind.Trim().ToLowerInvariant() switch
        {
            "image" => ConversationAttachmentKind.Image,
            "audio" => ConversationAttachmentKind.Audio,
            _ => ConversationAttachmentKind.File,
        };
}
