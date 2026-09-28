using System.Net;
using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Text;
using System.Text.Json;
using ChatOS.Api.Http;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Media;

public sealed partial class MediaGenerationService
{
    private const int MaximumVideoJobResponseBytes = 2 * 1024 * 1024;
    private const int MaximumVideoBytes = 512 * 1024 * 1024;
    private const int MaximumReferenceVideoBytes = 47 * 1024 * 1024;
    private static readonly TimeSpan VideoRequestTimeout = TimeSpan.FromMinutes(10);

    public Task<VideoGenerationResult> GenerateVideoAsync(
        VideoGenerationRequest request,
        IProgress<VideoGenerationProgress>? progress = null,
        CancellationToken cancellationToken = default) =>
        RunVideoAsync(request, null, progress, cancellationToken);

    public Task<VideoGenerationResult> ResumeVideoAsync(
        VideoGenerationRequest request,
        string jobId,
        IProgress<VideoGenerationProgress>? progress = null,
        CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(jobId))
            throw new ArgumentException("A video job ID is required.", nameof(jobId));
        return RunVideoAsync(request, jobId.Trim(), progress, cancellationToken);
    }

    private async Task<VideoGenerationResult> RunVideoAsync(
        VideoGenerationRequest request,
        string? existingJobId,
        IProgress<VideoGenerationProgress>? progress,
        CancellationToken cancellationToken)
    {
        ValidateVideoRequest(request);
        var sessionToken = await _tokenStore.GetAccessTokenAsync(cancellationToken).ConfigureAwait(false);
        if (string.IsNullOrWhiteSpace(sessionToken))
            throw new ChatOSApiException("Sign in before generating media.");
        var runtime = await _client.GetAsync<RuntimeModelDto>(
            $"ai-model-configs/{Uri.EscapeDataString(request.ModelConfigId)}?include_secret=true",
            cancellationToken).ConfigureAwait(false);
        var baseUrl = NormalizeProviderBaseUrl(runtime);

        ProviderVideoJob job;
        if (existingJobId is null)
        {
            using var createRequest = BuildVideoCreateRequest(baseUrl, runtime, request);
            job = await SendVideoJobRequestAsync(
                createRequest, sessionToken, "create", cancellationToken).ConfigureAwait(false);
        }
        else
        {
            using var statusRequest = BuildVideoStatusRequest(baseUrl, runtime, existingJobId);
            job = await SendVideoJobRequestAsync(
                statusRequest, sessionToken, "query", cancellationToken).ConfigureAwait(false);
        }
        progress?.Report(new VideoGenerationProgress(job.Status, job.Percent, job.Id));

        var pollCount = 0;
        while (!job.IsCompleted && !job.IsFailed)
        {
            cancellationToken.ThrowIfCancellationRequested();
            if (pollCount++ >= _maximumVideoPollCount)
                throw new ChatOSApiException("Video generation timed out. You can retry it later.");
            if (_videoPollInterval > TimeSpan.Zero)
                await Task.Delay(_videoPollInterval, cancellationToken).ConfigureAwait(false);
            using var statusRequest = BuildVideoStatusRequest(baseUrl, runtime, job.Id);
            job = await SendVideoJobRequestAsync(
                statusRequest, sessionToken, "query", cancellationToken).ConfigureAwait(false);
            progress?.Report(new VideoGenerationProgress(job.Status, job.Percent, job.Id));
        }

        if (job.IsFailed)
            throw new ChatOSApiException($"Video generation failed: {job.ErrorMessage ?? "the provider did not include a reason"}");
        progress?.Report(new VideoGenerationProgress("downloading", null, job.Id));
        var video = await DownloadVideoAsync(
            baseUrl, runtime, job, sessionToken, cancellationToken).ConfigureAwait(false);
        return new VideoGenerationResult(
            job.Id,
            request.ModelConfigId,
            string.IsNullOrWhiteSpace(job.Model) ? runtime.Model!.Trim() : job.Model,
            DateTimeOffset.UtcNow,
            video.MimeType ?? "video/mp4",
            video.Body);
    }

    private async Task<ProviderVideoJob> SendVideoJobRequestAsync(
        HttpRequestMessage request,
        string sessionToken,
        string operation,
        CancellationToken cancellationToken)
    {
        var response = await SendProviderAsync(
            request,
            sessionToken,
            MaximumVideoJobResponseBytes,
            cancellationToken).ConfigureAwait(false);
        if (!response.IsSuccessStatusCode)
            throw new ChatOSApiException(
                $"Unable to {operation} the video job (HTTP {(int)response.StatusCode}): {ProviderError(response.Body)}",
                response.StatusCode);
        var prefix = Encoding.UTF8.GetString(response.Body.AsSpan(0, Math.Min(256, response.Body.Length)))
            .TrimStart();
        if (response.ContentType?.Contains("text/html", StringComparison.OrdinalIgnoreCase) == true ||
            prefix.StartsWith("<!doctype html", StringComparison.OrdinalIgnoreCase) ||
            prefix.StartsWith("<html", StringComparison.OrdinalIgnoreCase))
            throw new ChatOSApiException("The video endpoint returned a web page instead of job data.");
        return DecodeVideoJob(response.Body);
    }

    private async Task<ProviderResponse> DownloadVideoAsync(
        string baseUrl,
        RuntimeModelDto runtime,
        ProviderVideoJob job,
        string sessionToken,
        CancellationToken cancellationToken)
    {
        HttpRequestMessage request;
        if (job.HasMetadataUrl)
        {
            if (job.ContentUrl is not { Scheme: "https", Host.Length: > 0 } url ||
                !string.IsNullOrEmpty(url.UserInfo))
                throw new ChatOSApiException("The video provider returned an invalid download URL.");
            request = new HttpRequestMessage(HttpMethod.Get, url);
            request.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("video/mp4"));
        }
        else
        {
            request = new HttpRequestMessage(
                HttpMethod.Get,
                $"{baseUrl}/videos/{Uri.EscapeDataString(job.Id)}/content");
            AddProviderHeaders(request, runtime, "video/mp4");
        }
        using (request)
        {
            var response = await SendProviderAsync(
                request,
                sessionToken,
                MaximumVideoBytes,
                cancellationToken).ConfigureAwait(false);
            if (!response.IsSuccessStatusCode)
                throw new ChatOSApiException(
                    $"Unable to download the generated video (HTTP {(int)response.StatusCode}): {ProviderError(response.Body)}",
                    response.StatusCode);
            if (response.Body.Length == 0 ||
                response.ContentType?.Contains("text/html", StringComparison.OrdinalIgnoreCase) == true)
                throw new ChatOSApiException("The provider did not return a playable video file.");
            return response with
            {
                MimeType = response.ContentType?.Split(';')[0].Trim() is { Length: > 0 } type
                    ? type
                    : "video/mp4",
            };
        }
    }

    private async Task<ProviderResponse> SendProviderAsync(
        HttpRequestMessage request,
        string sessionToken,
        int maximumBytes,
        CancellationToken cancellationToken)
    {
        await EnsureSameSessionAsync(sessionToken, cancellationToken).ConfigureAwait(false);
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(VideoRequestTimeout);
        try
        {
            using var response = await _httpClientFactory.CreateClient(ProviderClientName)
                .SendAsync(request, HttpCompletionOption.ResponseHeadersRead, timeout.Token)
                .ConfigureAwait(false);
            var body = await ReadLimitedAsync(response.Content, maximumBytes, timeout.Token)
                .ConfigureAwait(false);
            await EnsureSameSessionAsync(sessionToken, cancellationToken).ConfigureAwait(false);
            return new ProviderResponse(
                response.StatusCode,
                response.Content.Headers.ContentType?.MediaType,
                null,
                body);
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            throw new ChatOSApiException("The video provider request timed out.");
        }
        catch (HttpRequestException exception)
        {
            throw new ChatOSApiException("Unable to connect to the video provider.", innerException: exception);
        }
    }

    private static HttpRequestMessage BuildVideoCreateRequest(
        string baseUrl,
        RuntimeModelDto runtime,
        VideoGenerationRequest request)
    {
        var profile = VideoGenerationProfile.ForModel(runtime.Model!);
        ValidateVideoRequest(request, profile);
        var metadata = new Dictionary<string, string>
        {
            ["ratio"] = request.ReferencePurpose == VideoGenerationReferencePurpose.Reference &&
                request.FirstFrame is null ? request.Ratio : "adaptive",
        };
        if (request.FirstFrame is not null)
            metadata["first_frame_image"] = DataUrl(request.FirstFrame);
        if (request.LastFrame is not null)
            metadata["last_frame_image"] = DataUrl(request.LastFrame);
        if (request.ReferenceAudio is not null)
            metadata["audio_url"] = DataUrl(request.ReferenceAudio.MimeType, request.ReferenceAudio.Base64Data);
        if (request.ReferenceVideo is not null)
            metadata["video_url"] = DataUrl(request.ReferenceVideo.MimeType, request.ReferenceVideo.Base64Data);
        var message = new HttpRequestMessage(HttpMethod.Post, $"{baseUrl}/videos")
        {
            Content = JsonContent.Create(new
            {
                model = runtime.Model,
                prompt = request.Prompt.Trim(),
                duration = request.Seconds,
                size = request.Size.ToLowerInvariant(),
                metadata,
            }, options: JsonOptions),
        };
        AddProviderHeaders(message, runtime);
        return message;
    }

    private static HttpRequestMessage BuildVideoStatusRequest(
        string baseUrl,
        RuntimeModelDto runtime,
        string jobId)
    {
        var message = new HttpRequestMessage(
            HttpMethod.Get,
            $"{baseUrl}/videos/{Uri.EscapeDataString(jobId)}");
        AddProviderHeaders(message, runtime);
        return message;
    }

    private static void AddProviderHeaders(
        HttpRequestMessage message,
        RuntimeModelDto runtime,
        string accept = "application/json")
    {
        message.Headers.Authorization = new AuthenticationHeaderValue("Bearer", runtime.ApiKey!.Trim());
        message.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue(accept));
    }

    private static ProviderVideoJob DecodeVideoJob(byte[] body)
    {
        try
        {
            using var document = JsonDocument.Parse(body);
            var root = document.RootElement;
            var id = StringValue(root, "task_id") ?? StringValue(root, "id");
            var status = StringValue(root, "status")?.ToLowerInvariant();
            if (string.IsNullOrWhiteSpace(id) || string.IsNullOrWhiteSpace(status))
                throw new JsonException();
            double? percent = null;
            if (root.TryGetProperty("progress", out var progress) &&
                progress.ValueKind == JsonValueKind.Number && progress.TryGetDouble(out var value))
                percent = value;
            string? errorMessage = null;
            var hasError = root.TryGetProperty("error", out var error) &&
                error.ValueKind is not (JsonValueKind.Null or JsonValueKind.Undefined);
            if (hasError)
            {
                errorMessage = error.ValueKind == JsonValueKind.String
                    ? error.GetString()?.Trim()
                    : StringValue(error, "message") ?? StringValue(error, "detail");
            }
            string? rawUrl = null;
            if (root.TryGetProperty("metadata", out var metadata) && metadata.ValueKind == JsonValueKind.Object)
                rawUrl = StringValue(metadata, "url");
            Uri.TryCreate(rawUrl, UriKind.Absolute, out var contentUrl);
            return new ProviderVideoJob(
                id.Trim(),
                status.Trim(),
                percent,
                StringValue(root, "model"),
                errorMessage,
                hasError,
                contentUrl,
                rawUrl is not null);
        }
        catch (JsonException exception)
        {
            throw new ChatOSApiException("The video provider returned invalid job data.", innerException: exception);
        }
    }

    private static string? StringValue(JsonElement parent, string name) =>
        parent.ValueKind == JsonValueKind.Object &&
        parent.TryGetProperty(name, out var value) &&
        value.ValueKind == JsonValueKind.String
            ? value.GetString()?.Trim()
            : null;

    private static string NormalizeProviderBaseUrl(RuntimeModelDto runtime)
    {
        if (runtime.Enabled == false || string.IsNullOrWhiteSpace(runtime.Model) ||
            string.IsNullOrWhiteSpace(runtime.ApiKey) || string.IsNullOrWhiteSpace(runtime.BaseUrl))
            throw new ChatOSApiException("The selected model is missing its API address or key.");
        var value = runtime.BaseUrl.Trim().TrimEnd('/');
        string[] suffixes = ["/images/generations", "/images/edits", "/chat/completions", "/responses"];
        var suffix = suffixes.FirstOrDefault(candidate =>
            value.EndsWith(candidate, StringComparison.OrdinalIgnoreCase));
        if (suffix is not null) value = value[..^suffix.Length];
        var videosIndex = value.LastIndexOf("/videos/", StringComparison.OrdinalIgnoreCase);
        if (videosIndex >= 0) value = value[..videosIndex];
        else if (value.EndsWith("/videos", StringComparison.OrdinalIgnoreCase)) value = value[..^7];
        if (!Uri.TryCreate(value, UriKind.Absolute, out var uri) ||
            uri.Scheme is not ("https" or "http"))
            throw new ChatOSApiException("The selected model has an invalid API address.");
        return value;
    }

    private static void ValidateVideoRequest(
        VideoGenerationRequest request,
        VideoGenerationProfile? profile = null)
    {
        if (string.IsNullOrWhiteSpace(request.ModelConfigId))
            throw new ArgumentException("Choose a video model.", nameof(request));
        if (string.IsNullOrWhiteSpace(request.Prompt) || request.Prompt.Length > 7_000)
            throw new ArgumentException("The video prompt is required and cannot exceed 7,000 characters.", nameof(request));
        if (profile is not null)
        {
            if (!profile.Sizes.Contains(request.Size, StringComparer.OrdinalIgnoreCase) ||
                !profile.Durations.Contains(request.Seconds))
                throw new ArgumentException("The selected video size or duration is not supported.", nameof(request));
            if (request.LastFrame is not null && (request.FirstFrame is null || !profile.SupportsLastFrame))
                throw new ArgumentException("The selected model cannot use a last frame.", nameof(request));
        }
        if (request.FirstFrame is null && !VideoGenerationProfile.Ratios.Contains(request.Ratio))
            throw new ArgumentException("Choose a supported video ratio.", nameof(request));
        if ((request.ReferenceAudio is not null || request.ReferenceVideo is not null) &&
            (request.FirstFrame is not null || request.LastFrame is not null))
            throw new ArgumentException("Reference video or audio cannot be combined with video frames.", nameof(request));
        if (request.ReferenceAudio is not null && request.ReferenceVideo is not null)
            throw new ArgumentException("Choose either reference video or reference audio, not both.", nameof(request));
        if (!Enum.IsDefined(request.ReferencePurpose))
            throw new ArgumentException("Choose a supported reference video purpose.", nameof(request));
        if (request.ReferencePurpose != VideoGenerationReferencePurpose.Reference &&
            request.ReferenceVideo is null)
            throw new ArgumentException("Editing or extending requires a reference video.", nameof(request));
        ValidateVideoFrame(request.FirstFrame, request);
        ValidateVideoFrame(request.LastFrame, request);
        ValidateReferenceVideo(request.ReferenceVideo, profile, request);
        ValidateReferenceAudio(request.ReferenceAudio, profile, request);
    }

    private static void ValidateVideoFrame(ImageGenerationInput? image, VideoGenerationRequest request)
    {
        if (image is null) return;
        if (image.MimeType.ToLowerInvariant() is not ("image/png" or "image/jpeg" or "image/webp"))
            throw new ArgumentException("Video frames must be PNG, JPEG, or WebP.", nameof(request));
        try
        {
            var bytes = Convert.FromBase64String(image.Base64Data);
            if (bytes.Length == 0 || bytes.Length > MaximumImageBytes)
                throw new ArgumentException("A video frame is empty or exceeds 20 MB.", nameof(request));
        }
        catch (FormatException)
        {
            throw new ArgumentException("A video frame is invalid.", nameof(request));
        }
    }

    private static string DataUrl(ImageGenerationInput image) =>
        DataUrl(image.MimeType, image.Base64Data);

    private static string DataUrl(string mimeType, string base64Data) =>
        $"data:{mimeType.ToLowerInvariant()};base64,{base64Data}";

    private static void ValidateReferenceAudio(
        VideoGenerationInputAudio? audio,
        VideoGenerationProfile? profile,
        VideoGenerationRequest request)
    {
        if (audio is null) return;
        if (profile is { SupportsReferenceVideo: false })
            throw new ArgumentException("The selected model cannot use reference audio.", nameof(request));
        string[] mimeTypes =
        [
            "audio/mpeg", "audio/mp3", "audio/wav", "audio/x-wav", "audio/vnd.wave",
            "audio/mp4", "audio/x-m4a", "audio/aac",
        ];
        if (!mimeTypes.Contains(audio.MimeType, StringComparer.OrdinalIgnoreCase))
            throw new ArgumentException("Reference audio must be MP3, WAV, M4A, or AAC.", nameof(request));
        try
        {
            var bytes = Convert.FromBase64String(audio.Base64Data);
            if (bytes.Length == 0 || bytes.Length > MaximumImageBytes)
                throw new ArgumentException("Reference audio is empty or exceeds 20 MB.", nameof(request));
        }
        catch (FormatException)
        {
            throw new ArgumentException("Reference audio is invalid.", nameof(request));
        }
    }

    private static void ValidateReferenceVideo(
        VideoGenerationInputVideo? video,
        VideoGenerationProfile? profile,
        VideoGenerationRequest request)
    {
        if (video is null) return;
        if (profile is { SupportsReferenceVideo: false })
            throw new ArgumentException("The selected model cannot use a reference video.", nameof(request));
        if (video.MimeType.ToLowerInvariant() is not ("video/mp4" or "video/quicktime"))
            throw new ArgumentException("Reference videos must be MP4 or MOV.", nameof(request));
        try
        {
            var bytes = Convert.FromBase64String(video.Base64Data);
            if (bytes.Length == 0 || bytes.Length > MaximumReferenceVideoBytes)
                throw new ArgumentException("A reference video is empty or exceeds 47 MB.", nameof(request));
        }
        catch (FormatException)
        {
            throw new ArgumentException("The reference video is invalid.", nameof(request));
        }
    }

    private sealed record ProviderVideoJob(
        string Id,
        string Status,
        double? Percent,
        string? Model,
        string? ErrorMessage,
        bool HasExplicitError,
        Uri? ContentUrl,
        bool HasMetadataUrl)
    {
        public bool IsCompleted => Status == "completed";
        public bool IsFailed => Status == "failed" || HasExplicitError;
    }

    private sealed record ProviderResponse(
        HttpStatusCode StatusCode,
        string? ContentType,
        string? MimeType,
        byte[] Body)
    {
        public bool IsSuccessStatusCode => (int)StatusCode is >= 200 and < 300;
    }
}
