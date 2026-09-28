using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Media;

public sealed class MediaGenerationService : IMediaGenerationService
{
    public const string ProviderClientName = "ChatOS.MediaGeneration.Provider";
    private const int MaximumImageBytes = 20 * 1024 * 1024;
    private const int MaximumResponseBytes = 48 * 1024 * 1024;
    private static readonly TimeSpan ProviderTimeout = TimeSpan.FromMinutes(10);
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = true,
    };

    private readonly ChatOSApiClient _client;
    private readonly IHttpClientFactory _httpClientFactory;
    private readonly IAuthTokenStore _tokenStore;

    public MediaGenerationService(
        ChatOSApiClient client,
        IHttpClientFactory httpClientFactory,
        IAuthTokenStore tokenStore)
    {
        _client = client;
        _httpClientFactory = httpClientFactory;
        _tokenStore = tokenStore;
    }

    public async Task<IReadOnlyList<MediaGenerationModel>> FetchModelsAsync(
        CancellationToken cancellationToken = default)
    {
        var models = await _client.GetAsync<IReadOnlyList<MediaModelDto>>(
            "ai-model-configs",
            cancellationToken).ConfigureAwait(false);
        return models
            .Where(model => model.Enabled != false && model.HasApiKey != false)
            .Select(model => model.ToDomain())
            .OrderBy(model => model.IsLikelyVideoModel)
            .ThenBy(model => model.Name, StringComparer.CurrentCultureIgnoreCase)
            .ToArray();
    }

    public async Task<ImageGenerationResult> GenerateImageAsync(
        ImageGenerationRequest request,
        CancellationToken cancellationToken = default)
    {
        Validate(request);
        var sessionToken = await _tokenStore.GetAccessTokenAsync(cancellationToken)
            .ConfigureAwait(false);
        if (string.IsNullOrWhiteSpace(sessionToken))
            throw new ChatOSApiException("Sign in before generating media.");

        var runtime = await _client.GetAsync<RuntimeModelDto>(
            $"ai-model-configs/{Uri.EscapeDataString(request.ModelConfigId)}?include_secret=true",
            cancellationToken).ConfigureAwait(false);
        var endpoint = ProviderEndpoint(runtime, request.ReferenceImages.Count > 0);
        using var providerRequest = BuildProviderRequest(endpoint, runtime, request);
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(ProviderTimeout);
        HttpResponseMessage response;
        try
        {
            response = await _httpClientFactory.CreateClient(ProviderClientName)
                .SendAsync(providerRequest, HttpCompletionOption.ResponseHeadersRead, timeout.Token)
                .ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            throw new ChatOSApiException("The image provider request timed out.");
        }
        catch (HttpRequestException exception)
        {
            throw new ChatOSApiException("Unable to connect to the image provider.", innerException: exception);
        }
        using (response)
        {
            var payload = await ReadLimitedAsync(response.Content, MaximumResponseBytes, timeout.Token)
                .ConfigureAwait(false);
            await EnsureSameSessionAsync(sessionToken, cancellationToken).ConfigureAwait(false);
            if (!response.IsSuccessStatusCode)
            {
                throw new ChatOSApiException(
                    $"Image provider rejected the request (HTTP {(int)response.StatusCode}): {ProviderError(payload)}",
                    response.StatusCode);
            }

            return DecodeResult(payload, request, runtime);
        }
    }

    private static ImageGenerationResult DecodeResult(
        byte[] payload,
        ImageGenerationRequest request,
        RuntimeModelDto runtime)
    {
        ProviderImageResponse provider;
        try
        {
            provider = JsonSerializer.Deserialize<ProviderImageResponse>(payload, JsonOptions)
                ?? throw new JsonException();
        }
        catch (JsonException exception)
        {
            throw new ChatOSApiException("The image provider returned an invalid response.", innerException: exception);
        }

        var resultId = provider.Id?.Trim();
        if (string.IsNullOrEmpty(resultId)) resultId = $"media_{Guid.NewGuid():N}";
        var images = (provider.Data ?? [])
            .Select((item, index) => MapAsset(item, resultId, index))
            .ToArray();
        if (images.Length == 0)
            throw new ChatOSApiException("The image provider returned no images.");
        return new ImageGenerationResult(
            resultId,
            request.ModelConfigId,
            string.IsNullOrWhiteSpace(provider.Model) ? runtime.Model!.Trim() : provider.Model.Trim(),
            DateTimeOffset.UtcNow,
            images);
    }

    private async Task EnsureSameSessionAsync(
        string expectedToken,
        CancellationToken cancellationToken)
    {
        var current = await _tokenStore.GetAccessTokenAsync(cancellationToken).ConfigureAwait(false);
        if (!string.Equals(current, expectedToken, StringComparison.Ordinal))
            throw new ChatOSApiException("The signed-in account changed while media was generating.");
    }

    private static HttpRequestMessage BuildProviderRequest(
        Uri endpoint,
        RuntimeModelDto runtime,
        ImageGenerationRequest request)
    {
        var message = new HttpRequestMessage(HttpMethod.Post, endpoint);
        message.Headers.Authorization = new AuthenticationHeaderValue("Bearer", runtime.ApiKey!.Trim());
        message.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("application/json"));
        if (request.ReferenceImages.Count == 0)
        {
            message.Content = JsonContent.Create(new
            {
                model = runtime.Model,
                prompt = request.Prompt.Trim(),
                n = request.Count,
                size = request.Size,
            }, options: JsonOptions);
            return message;
        }

        var multipart = new MultipartFormDataContent($"ChatOSMediaBoundary{Guid.NewGuid():N}");
        multipart.Add(new StringContent(runtime.Model!), "model");
        multipart.Add(new StringContent(request.Prompt.Trim()), "prompt");
        multipart.Add(new StringContent(request.Count.ToString()), "n");
        if (!string.IsNullOrWhiteSpace(request.Size))
            multipart.Add(new StringContent(request.Size), "size");
        foreach (var image in request.ReferenceImages)
        {
            var bytes = Convert.FromBase64String(image.Base64Data);
            var content = new ByteArrayContent(bytes);
            content.Headers.ContentType = new MediaTypeHeaderValue(image.MimeType.ToLowerInvariant());
            multipart.Add(
                content,
                request.ReferenceImages.Count > 1 ? "image[]" : "image",
                SafeFileName(image));
        }
        message.Content = multipart;
        return message;
    }

    private static GeneratedMediaAsset MapAsset(ProviderImageItem item, string resultId, int index)
    {
        var base64 = item.Base64Json ?? item.Base64Data;
        if (!string.IsNullOrWhiteSpace(base64))
        {
            byte[] bytes;
            try { bytes = Convert.FromBase64String(base64); }
            catch (FormatException exception)
            {
                throw new ChatOSApiException("The image provider returned invalid image data.", innerException: exception);
            }
            if (bytes.Length == 0 || bytes.Length > MaximumImageBytes)
                throw new ChatOSApiException("The generated image is empty or exceeds 20 MB.");
            return new GeneratedMediaAsset(
                item.Id ?? $"{resultId}:image:{index}",
                item.MimeType ?? "image/png",
                base64,
                null,
                item.RevisedPrompt);
        }
        if (Uri.TryCreate(item.Url, UriKind.Absolute, out var url) && url.Scheme == Uri.UriSchemeHttps)
        {
            return new GeneratedMediaAsset(
                item.Id ?? $"{resultId}:image:{index}",
                item.MimeType ?? "image/png",
                null,
                url,
                item.RevisedPrompt);
        }
        throw new ChatOSApiException("The image provider returned an unsupported image location.");
    }

    private static Uri ProviderEndpoint(RuntimeModelDto runtime, bool edit)
    {
        if (runtime.Enabled == false || string.IsNullOrWhiteSpace(runtime.Model) ||
            string.IsNullOrWhiteSpace(runtime.ApiKey) || string.IsNullOrWhiteSpace(runtime.BaseUrl))
            throw new ChatOSApiException("The selected model is missing its API address or key.");
        var value = runtime.BaseUrl.Trim().TrimEnd('/');
        string[] suffixes = ["/images/generations", "/images/edits", "/chat/completions", "/responses"];
        var suffix = suffixes.FirstOrDefault(candidate =>
            value.EndsWith(candidate, StringComparison.OrdinalIgnoreCase));
        if (suffix is not null) value = value[..^suffix.Length];
        if (!Uri.TryCreate($"{value}/images/{(edit ? "edits" : "generations")}", UriKind.Absolute, out var uri) ||
            uri.Scheme is not (Uri.UriSchemeHttps or Uri.UriSchemeHttp))
            throw new ChatOSApiException("The selected model has an invalid API address.");
        return uri;
    }

    private static void Validate(ImageGenerationRequest request)
    {
        if (string.IsNullOrWhiteSpace(request.ModelConfigId))
            throw new ArgumentException("Choose an image model.", nameof(request));
        if (string.IsNullOrWhiteSpace(request.Prompt) || request.Prompt.Length > 32_000)
            throw new ArgumentException("The prompt is required and cannot exceed 32,000 characters.", nameof(request));
        if (request.Count is < 1 or > 4)
            throw new ArgumentOutOfRangeException(nameof(request), "Generate between one and four images.");
        if (request.ReferenceImages.Count > 8)
            throw new ArgumentException("At most eight reference images can be used.", nameof(request));
        foreach (var image in request.ReferenceImages)
        {
            if (image.MimeType.ToLowerInvariant() is not ("image/png" or "image/jpeg" or "image/webp"))
                throw new ArgumentException("Reference images must be PNG, JPEG, or WebP.", nameof(request));
            byte[] bytes;
            try { bytes = Convert.FromBase64String(image.Base64Data); }
            catch (FormatException) { throw new ArgumentException("A reference image is invalid.", nameof(request)); }
            if (bytes.Length == 0 || bytes.Length > MaximumImageBytes)
                throw new ArgumentException("A reference image is empty or exceeds 20 MB.", nameof(request));
        }
    }

    private static string SafeFileName(ImageGenerationInput image)
    {
        var extension = image.MimeType.ToLowerInvariant() switch
        {
            "image/jpeg" => ".jpg",
            "image/webp" => ".webp",
            _ => ".png",
        };
        var stem = Path.GetFileNameWithoutExtension(image.Name);
        stem = new string(stem.Where(character => char.IsAsciiLetterOrDigit(character) || character is '-' or '_').ToArray());
        return $"{(stem.Length == 0 ? "input" : stem)}{extension}";
    }

    private static async Task<byte[]> ReadLimitedAsync(
        HttpContent content,
        int maximumBytes,
        CancellationToken cancellationToken)
    {
        await using var input = await content.ReadAsStreamAsync(cancellationToken).ConfigureAwait(false);
        using var output = new MemoryStream();
        var buffer = new byte[81920];
        while (true)
        {
            var read = await input.ReadAsync(buffer, cancellationToken).ConfigureAwait(false);
            if (read == 0) break;
            if (output.Length + read > maximumBytes)
                throw new ChatOSApiException("The image provider response exceeds 48 MB.");
            output.Write(buffer, 0, read);
        }
        return output.ToArray();
    }

    private static string ProviderError(byte[] payload)
    {
        try
        {
            using var document = JsonDocument.Parse(payload);
            if (document.RootElement.TryGetProperty("error", out var error))
            {
                if (error.ValueKind == JsonValueKind.String) return error.GetString()!;
                if (error.TryGetProperty("message", out var nested)) return nested.GetString() ?? "Unknown provider error";
            }
            if (document.RootElement.TryGetProperty("message", out var message))
                return message.GetString() ?? "Unknown provider error";
        }
        catch (JsonException) { }
        var raw = Encoding.UTF8.GetString(payload.AsSpan(0, Math.Min(payload.Length, 2000))).Trim();
        return raw.Length == 0 ? "The provider returned an empty error response." : raw;
    }

    private sealed record MediaModelDto
    {
        [JsonPropertyName("id")] public required string Id { get; init; }
        [JsonPropertyName("name")] public required string Name { get; init; }
        [JsonPropertyName("provider")] public string? Provider { get; init; }
        [JsonPropertyName("model")] public string? Model { get; init; }
        [JsonPropertyName("enabled")] public bool? Enabled { get; init; }
        [JsonPropertyName("has_api_key")] public bool? HasApiKey { get; init; }
        public MediaGenerationModel ToDomain() => new(
            Id, Name, Provider ?? string.Empty, Model ?? Name, Enabled != false, HasApiKey != false);
    }

    private sealed record RuntimeModelDto(
        [property: JsonPropertyName("model")] string? Model,
        [property: JsonPropertyName("api_key")] string? ApiKey,
        [property: JsonPropertyName("base_url")] string? BaseUrl,
        [property: JsonPropertyName("enabled")] bool? Enabled);

    private sealed record ProviderImageResponse(
        [property: JsonPropertyName("id")] string? Id,
        [property: JsonPropertyName("model")] string? Model,
        [property: JsonPropertyName("data")] IReadOnlyList<ProviderImageItem>? Data);

    private sealed record ProviderImageItem(
        [property: JsonPropertyName("id")] string? Id,
        [property: JsonPropertyName("b64_json")] string? Base64Json,
        [property: JsonPropertyName("base64")] string? Base64Data,
        [property: JsonPropertyName("url")] string? Url,
        [property: JsonPropertyName("mime_type")] string? MimeType,
        [property: JsonPropertyName("revised_prompt")] string? RevisedPrompt);
}
