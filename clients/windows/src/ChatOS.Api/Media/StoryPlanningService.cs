using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Media;

public sealed class StoryPlanningService(
    ChatOSApiClient client,
    IHttpClientFactory httpClientFactory,
    IAuthTokenStore tokenStore) : IStoryPlanningService
{
    public const string ProviderClientName = "ChatOS.StoryPlanning.Provider";
    private const int MaximumResponseBytes = 4 * 1024 * 1024;
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = true,
    };

    public async Task<StoryPlanningResult> PlanAsync(
        StoryPlanningRequest request,
        CancellationToken cancellationToken = default)
    {
        ValidateRequest(request);
        var payload = await SendStructuredAsync(
            request.ModelConfigId,
            runtime => BuildBody(runtime, request),
            cancellationToken).ConfigureAwait(false);
        return Decode(payload, request.MaximumSegments);
    }

    public async Task<StoryOptimizationSuggestion> OptimizeAsync(
        StoryOptimizationRequest request,
        CancellationToken cancellationToken = default)
    {
        ValidateOptimizationRequest(request);
        var payload = await SendStructuredAsync(
            request.ModelConfigId,
            runtime => BuildOptimizationBody(runtime, request),
            cancellationToken).ConfigureAwait(false);
        return DecodeOptimization(payload, request.Target);
    }

    public async Task<StorySegmentRefinementSuggestion> RefineSegmentAsync(
        StorySegmentRefinementRequest request,
        CancellationToken cancellationToken = default)
    {
        ValidateSegmentRefinementRequest(request);
        var payload = await SendStructuredAsync(
            request.ModelConfigId,
            runtime => BuildSegmentRefinementBody(runtime, request),
            cancellationToken).ConfigureAwait(false);
        return DecodeSegmentRefinement(payload);
    }

    private async Task<byte[]> SendStructuredAsync(
        string modelConfigId,
        Func<RuntimeModelDto, object> body,
        CancellationToken cancellationToken)
    {
        var sessionToken = await tokenStore.GetAccessTokenAsync(cancellationToken).ConfigureAwait(false);
        if (string.IsNullOrWhiteSpace(sessionToken))
            throw new ChatOSApiException("Sign in before planning a story.");
        var runtime = await client.GetUserServiceAsync<RuntimeModelDto>(
            $"model-configs/{Uri.EscapeDataString(modelConfigId)}?include_secret=true",
            cancellationToken).ConfigureAwait(false);
        var endpoint = ResponsesEndpoint(runtime);
        using var providerRequest = new HttpRequestMessage(HttpMethod.Post, endpoint)
        {
            Content = JsonContent.Create(body(runtime), options: JsonOptions),
        };
        providerRequest.Headers.Authorization = new AuthenticationHeaderValue("Bearer", runtime.ApiKey!.Trim());
        providerRequest.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("application/json"));
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(TimeSpan.FromMinutes(5));
        try
        {
            using var response = await httpClientFactory.CreateClient(ProviderClientName)
                .SendAsync(providerRequest, HttpCompletionOption.ResponseHeadersRead, timeout.Token)
                .ConfigureAwait(false);
            var payload = await ReadLimitedAsync(response.Content, timeout.Token).ConfigureAwait(false);
            var currentToken = await tokenStore.GetAccessTokenAsync(cancellationToken).ConfigureAwait(false);
            if (!string.Equals(sessionToken, currentToken, StringComparison.Ordinal))
                throw new ChatOSApiException("The signed-in account changed while the story was being planned.");
            if (!response.IsSuccessStatusCode)
                throw new ChatOSApiException(
                    $"Story planning provider rejected the request (HTTP {(int)response.StatusCode}): {ProviderError(payload)}",
                    response.StatusCode);
            return payload;
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            throw new ChatOSApiException("The story planning request timed out.");
        }
        catch (HttpRequestException exception)
        {
            throw new ChatOSApiException("Unable to connect to the story planning provider.", innerException: exception);
        }
    }

    private static object BuildBody(RuntimeModelDto runtime, StoryPlanningRequest request) => new
    {
        model = runtime.Model,
        input = new object[]
        {
            new
            {
                role = "developer",
                content = StoryPromptCatalog.PlanningSystem,
            },
            new
            {
                role = "user",
                content = StoryPromptCatalog.RenderPlanningUser(request),
            },
        },
        text = new
        {
            format = new
            {
                type = "json_schema",
                name = "story_plan",
                strict = true,
                schema = Schema(),
            },
        },
        max_output_tokens = 32_000,
        store = false,
    };

    private static object BuildOptimizationBody(RuntimeModelDto runtime, StoryOptimizationRequest request) => new
    {
        model = runtime.Model,
        input = new object[]
        {
            new
            {
                role = "developer",
                content = request.Target == StoryOptimizationTarget.Source
                    ? StoryPromptCatalog.OptimizeSourceSystem
                    : StoryPromptCatalog.OptimizeStyleSystem,
            },
            new
            {
                role = "user",
                content = StoryPromptCatalog.RenderOptimizationUser(request),
            },
        },
        text = new
        {
            format = new
            {
                type = "json_schema",
                name = "story_optimization",
                strict = true,
                schema = new
                {
                    type = "object",
                    additionalProperties = false,
                    required = new[] { "optimized_text", "rationale" },
                    properties = new
                    {
                        optimized_text = new
                        {
                            type = "string",
                            minLength = 1,
                            maxLength = request.Target == StoryOptimizationTarget.Source ? 80_000 : 2_000,
                        },
                        rationale = new { type = "string", minLength = 1, maxLength = 2_000 },
                    },
                },
            },
        },
        max_output_tokens = request.Target == StoryOptimizationTarget.Source ? 32_000 : 4_000,
        store = false,
    };

    private static object BuildSegmentRefinementBody(
        RuntimeModelDto runtime,
        StorySegmentRefinementRequest request) => new
    {
        model = runtime.Model,
        input = new object[]
        {
            new { role = "developer", content = StoryPromptCatalog.RefineSegmentSystem },
            new { role = "user", content = StoryPromptCatalog.RenderSegmentRefinementUser(request) },
        },
        text = new
        {
            format = new
            {
                type = "json_schema",
                name = "story_segment_refinement",
                strict = true,
                schema = new
                {
                    type = "object",
                    additionalProperties = false,
                    required = new[]
                    {
                        "image_prompt", "video_prompt", "continuity_in",
                        "continuity_out", "shot_plan", "rationale",
                    },
                    properties = new
                    {
                        image_prompt = new { type = "string", minLength = 1, maxLength = 7_000 },
                        video_prompt = new { type = "string", minLength = 1, maxLength = 7_000 },
                        continuity_in = new { type = "string", minLength = 1, maxLength = 2_000 },
                        continuity_out = new { type = "string", minLength = 1, maxLength = 2_000 },
                        shot_plan = new { type = "string", minLength = 1, maxLength = 8_000 },
                        rationale = new { type = "string", minLength = 1, maxLength = 2_000 },
                    },
                },
            },
        },
        max_output_tokens = 12_000,
        store = false,
    };

    private static object Schema() => new
    {
        type = "object",
        additionalProperties = false,
        required = new[] { "summary", "resources", "segments" },
        properties = new
        {
            summary = new { type = "string", maxLength = 16000 },
            resources = new
            {
                type = "array",
                maxItems = 100,
                items = new
                {
                    type = "object",
                    additionalProperties = false,
                    required = new[] { "id", "kind", "name", "description", "image_prompt" },
                    properties = new
                    {
                        id = new { type = "string", minLength = 1, maxLength = 80 },
                        kind = new { type = "string", @enum = new[] { "character", "scene", "prop" } },
                        name = new { type = "string", minLength = 1, maxLength = 200 },
                        description = new { type = "string", maxLength = 8000 },
                        image_prompt = new { type = "string", minLength = 1, maxLength = 7000 },
                    },
                },
            },
            segments = new
            {
                type = "array",
                minItems = 1,
                maxItems = 200,
                items = new
                {
                    type = "object",
                    additionalProperties = false,
                    required = new[] { "kind", "title", "narrative", "image_prompt", "video_prompt", "seconds", "resource_ids" },
                    properties = new
                    {
                        kind = new { type = "string", @enum = new[] { "story", "transition" } },
                        title = new { type = "string", minLength = 1, maxLength = 200 },
                        narrative = new { type = "string", minLength = 1, maxLength = 8000 },
                        image_prompt = new { type = "string", minLength = 1, maxLength = 7000 },
                        video_prompt = new { type = "string", minLength = 1, maxLength = 7000 },
                        seconds = new { type = "integer", minimum = 2, maximum = 15 },
                        resource_ids = new
                        {
                            type = "array",
                            uniqueItems = true,
                            items = new { type = "string", minLength = 1, maxLength = 80 },
                        },
                    },
                },
            },
        },
    };

    private static StoryPlanningResult Decode(byte[] payload, int maximumSegments)
    {
        try
        {
            using var response = JsonDocument.Parse(payload);
            var text = OutputText(response.RootElement);
            if (string.IsNullOrWhiteSpace(text)) throw new JsonException("Missing output text.");
            var trimmed = StripCodeFence(text);
            var plan = JsonSerializer.Deserialize<PlanDto>(trimmed, JsonOptions) ?? throw new JsonException();
            if (plan.Segments is not { Count: > 0 } || plan.Segments.Count > maximumSegments)
                throw new JsonException("Invalid segment count.");
            var resources = (plan.Resources ?? []).Select(resource => new PlannedStoryResource(
                Identifier(resource.Id),
                ResourceKind(resource.Kind),
                Required(resource.Name, 200),
                Optional(resource.Description, 8_000),
                Required(resource.ImagePrompt, 7_000)))
                .ToArray();
            if (resources.Length > 100 || resources.Select(resource => resource.Id).Distinct(StringComparer.Ordinal).Count() != resources.Length)
                throw new JsonException("Invalid resources.");
            var resourceIds = resources.Select(resource => resource.Id).ToHashSet(StringComparer.Ordinal);
            var segments = plan.Segments.Select(segment => new PlannedStorySegment(
                SegmentKind(segment.Kind),
                Required(segment.Title, 200),
                Required(segment.Narrative, 8_000),
                Required(segment.ImagePrompt, 7_000),
                Required(segment.VideoPrompt, 7_000),
                segment.Seconds is >= 2 and <= 15 ? segment.Seconds : throw new JsonException("Invalid duration."),
                ResourceIds(segment.ResourceIds, resourceIds)))
                .ToArray();
            var summary = (plan.Summary ?? string.Empty).Trim();
            if (summary.Length > 16_000) throw new JsonException("Summary is too long.");
            return new StoryPlanningResult(summary, resources, segments);
        }
        catch (JsonException exception)
        {
            throw new ChatOSApiException("The story planning provider returned an invalid plan.", innerException: exception);
        }
    }

    private static StoryOptimizationSuggestion DecodeOptimization(
        byte[] payload,
        StoryOptimizationTarget target)
    {
        try
        {
            using var response = JsonDocument.Parse(payload);
            var text = OutputText(response.RootElement);
            if (string.IsNullOrWhiteSpace(text)) throw new JsonException("Missing output text.");
            var suggestion = JsonSerializer.Deserialize<OptimizationDto>(StripCodeFence(text), JsonOptions)
                ?? throw new JsonException();
            var maximumLength = target == StoryOptimizationTarget.Source ? 80_000 : 2_000;
            return new StoryOptimizationSuggestion(
                Required(suggestion.OptimizedText, maximumLength),
                Required(suggestion.Rationale, 2_000));
        }
        catch (JsonException exception)
        {
            throw new ChatOSApiException(
                "The story planning provider returned an invalid optimization suggestion.",
                innerException: exception);
        }
    }

    private static StorySegmentRefinementSuggestion DecodeSegmentRefinement(byte[] payload)
    {
        try
        {
            using var response = JsonDocument.Parse(payload);
            var text = OutputText(response.RootElement);
            if (string.IsNullOrWhiteSpace(text)) throw new JsonException("Missing output text.");
            var suggestion = JsonSerializer.Deserialize<SegmentRefinementDto>(StripCodeFence(text), JsonOptions)
                ?? throw new JsonException();
            return new StorySegmentRefinementSuggestion(
                Required(suggestion.ImagePrompt, 7_000),
                Required(suggestion.VideoPrompt, 7_000),
                Required(suggestion.ContinuityIn, 2_000),
                Required(suggestion.ContinuityOut, 2_000),
                Required(suggestion.ShotPlan, 8_000),
                Required(suggestion.Rationale, 2_000));
        }
        catch (JsonException exception)
        {
            throw new ChatOSApiException(
                "The story planning provider returned an invalid segment refinement.",
                innerException: exception);
        }
    }

    private static string? OutputText(JsonElement root)
    {
        if (root.TryGetProperty("output_text", out var direct) && direct.ValueKind == JsonValueKind.String)
            return direct.GetString();
        if (!root.TryGetProperty("output", out var output) || output.ValueKind != JsonValueKind.Array) return null;
        var parts = new List<string>();
        foreach (var item in output.EnumerateArray())
        {
            if (!item.TryGetProperty("content", out var content) || content.ValueKind != JsonValueKind.Array) continue;
            foreach (var part in content.EnumerateArray())
            {
                if (part.TryGetProperty("text", out var text) && text.ValueKind == JsonValueKind.String)
                    parts.Add(text.GetString()!);
            }
        }
        return string.Join(string.Empty, parts);
    }

    private static Uri ResponsesEndpoint(RuntimeModelDto runtime)
    {
        if (runtime.Enabled == false || string.IsNullOrWhiteSpace(runtime.Model) ||
            string.IsNullOrWhiteSpace(runtime.ApiKey) ||
            !Uri.TryCreate(runtime.BaseUrl?.Trim(), UriKind.Absolute, out var baseUri) ||
            baseUri.Scheme is not ("http" or "https") || string.IsNullOrWhiteSpace(baseUri.Host) ||
            !string.IsNullOrEmpty(baseUri.UserInfo))
            throw new ChatOSApiException("The selected text model is missing a valid API address or key.");
        var path = baseUri.AbsolutePath.TrimEnd('/');
        string[] suffixes = ["/chat/completions", "/responses"];
        var suffix = suffixes.FirstOrDefault(value => path.EndsWith(value, StringComparison.OrdinalIgnoreCase));
        if (suffix is not null) path = path[..^suffix.Length];
        return new UriBuilder(baseUri) { Path = $"{path}/responses", Query = string.Empty, Fragment = string.Empty }.Uri;
    }

    private static void ValidateRequest(StoryPlanningRequest request)
    {
        if (string.IsNullOrWhiteSpace(request.ModelConfigId)) throw new ArgumentException("Choose a text model.", nameof(request));
        if (string.IsNullOrWhiteSpace(request.Title) || request.Title.Length > 120) throw new ArgumentException("The story title is invalid.", nameof(request));
        if (string.IsNullOrWhiteSpace(request.Source) || request.Source.Length > 80_000) throw new ArgumentException("The story source is required and cannot exceed 80,000 characters.", nameof(request));
        if (request.Description.Length > 4_000 || request.VisualStyle.Length > 2_000 ||
            request.CreativeRequirements is null or { Length: > 2_000 })
            throw new ArgumentException("The story description, style, or creative requirements are too long.", nameof(request));
        if (!StoryPlanningRatios.Contains(request.Ratio) || request.MaximumSegments is < 1 or > 200) throw new ArgumentException("The story ratio or segment limit is invalid.", nameof(request));
    }

    private static void ValidateOptimizationRequest(StoryOptimizationRequest request)
    {
        if (string.IsNullOrWhiteSpace(request.ModelConfigId))
            throw new ArgumentException("Choose a text model.", nameof(request));
        if (string.IsNullOrWhiteSpace(request.Title) || request.Title.Length > 120 ||
            request.Description is null or { Length: > 4_000 } ||
            request.Source is null or { Length: > 80_000 } ||
            request.VisualStyle is null or { Length: > 2_000 } ||
            !Enum.IsDefined(request.Target))
            throw new ArgumentException("The story optimization input is invalid.", nameof(request));
        var selected = request.Target == StoryOptimizationTarget.Source
            ? request.Source
            : request.VisualStyle;
        if (string.IsNullOrWhiteSpace(selected))
            throw new ArgumentException("The text to optimize is required.", nameof(request));
    }

    private static void ValidateSegmentRefinementRequest(StorySegmentRefinementRequest request)
    {
        if (string.IsNullOrWhiteSpace(request.ModelConfigId) ||
            string.IsNullOrWhiteSpace(request.ProjectTitle) || request.ProjectTitle.Length > 120 ||
            request.ProjectSummary is null or { Length: > 16_000 } ||
            request.VisualStyle is null or { Length: > 2_000 } ||
            !StoryPlanningRatios.Contains(request.Ratio) ||
            string.IsNullOrWhiteSpace(request.SegmentId) || request.SegmentId.Length > 80 ||
            request.Kind is not ("story" or "transition") ||
            string.IsNullOrWhiteSpace(request.Title) || request.Title.Length > 200 ||
            string.IsNullOrWhiteSpace(request.Narrative) || request.Narrative.Length > 8_000 ||
            request.Seconds is < 2 or > 15 ||
            request.ImagePrompt is null or { Length: > 7_000 } ||
            request.VideoPrompt is null or { Length: > 7_000 } ||
            request.ContinuityContext is null or { Length: > 8_000 } ||
            request.ResourceContext is null or { Length: > 16_000 } ||
            request.CreativeRequirements is null or { Length: > 2_000 })
            throw new ArgumentException("The segment refinement input is invalid.", nameof(request));
    }

    private static async Task<byte[]> ReadLimitedAsync(HttpContent content, CancellationToken cancellationToken)
    {
        await using var input = await content.ReadAsStreamAsync(cancellationToken).ConfigureAwait(false);
        using var output = new MemoryStream();
        var buffer = new byte[16 * 1024];
        while (true)
        {
            var read = await input.ReadAsync(buffer, cancellationToken).ConfigureAwait(false);
            if (read == 0) break;
            if (output.Length + read > MaximumResponseBytes) throw new ChatOSApiException("The story planning response exceeds 4 MB.");
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
        }
        catch (JsonException) { }
        var raw = Encoding.UTF8.GetString(payload.AsSpan(0, Math.Min(payload.Length, 2000))).Trim();
        return raw.Length == 0 ? "The provider returned an empty error response." : raw;
    }

    private static string Required(string? value, int maximumLength)
    {
        var result = value?.Trim();
        return !string.IsNullOrWhiteSpace(result) && result.Length <= maximumLength
            ? result
            : throw new JsonException("A required story plan field is invalid.");
    }

    private static string Optional(string? value, int maximumLength)
    {
        var result = value?.Trim() ?? string.Empty;
        return result.Length <= maximumLength ? result : throw new JsonException("A story plan field is too long.");
    }

    private static string Identifier(string? value)
    {
        var result = Required(value, 80);
        return result.All(character => char.IsAsciiLetterOrDigit(character) || character is '-' or '_')
            ? result
            : throw new JsonException("A resource ID is invalid.");
    }

    private static string ResourceKind(string? value) => value?.Trim().ToLowerInvariant() switch
    {
        "character" => "character",
        "scene" => "scene",
        "prop" => "prop",
        _ => throw new JsonException("A resource kind is invalid."),
    };

    private static string SegmentKind(string? value) => value?.Trim().ToLowerInvariant() switch
    {
        "story" => "story",
        "transition" => "transition",
        _ => throw new JsonException("A segment kind is invalid."),
    };

    private static IReadOnlyList<string> ResourceIds(
        IReadOnlyList<string>? values,
        IReadOnlySet<string> known)
    {
        var ids = (values ?? []).Select(Identifier).ToArray();
        if (ids.Distinct(StringComparer.Ordinal).Count() != ids.Length || ids.Any(id => !known.Contains(id)))
            throw new JsonException("A segment contains invalid resource references.");
        return ids;
    }

    private static string StripCodeFence(string value)
    {
        var trimmed = value.Trim();
        if (!trimmed.StartsWith("```", StringComparison.Ordinal)) return trimmed;
        var firstLine = trimmed.IndexOf('\n');
        var lastFence = trimmed.LastIndexOf("```", StringComparison.Ordinal);
        return firstLine >= 0 && lastFence > firstLine ? trimmed[(firstLine + 1)..lastFence].Trim() : trimmed;
    }

    private static readonly string[] StoryPlanningRatios = ["16:9", "9:16", "1:1", "4:3", "3:4", "21:9"];

    private sealed record RuntimeModelDto(
        [property: JsonPropertyName("model")] string? Model,
        [property: JsonPropertyName("api_key")] string? ApiKey,
        [property: JsonPropertyName("base_url")] string? BaseUrl,
        [property: JsonPropertyName("enabled")] bool? Enabled);

    private sealed record PlanDto(
        [property: JsonPropertyName("summary")] string? Summary,
        [property: JsonPropertyName("resources")] IReadOnlyList<ResourceDto>? Resources,
        [property: JsonPropertyName("segments")] IReadOnlyList<SegmentDto>? Segments);

    private sealed record ResourceDto(
        [property: JsonPropertyName("id")] string? Id,
        [property: JsonPropertyName("kind")] string? Kind,
        [property: JsonPropertyName("name")] string? Name,
        [property: JsonPropertyName("description")] string? Description,
        [property: JsonPropertyName("image_prompt")] string? ImagePrompt);

    private sealed record SegmentDto(
        [property: JsonPropertyName("kind")] string? Kind,
        [property: JsonPropertyName("title")] string? Title,
        [property: JsonPropertyName("narrative")] string? Narrative,
        [property: JsonPropertyName("image_prompt")] string? ImagePrompt,
        [property: JsonPropertyName("video_prompt")] string? VideoPrompt,
        [property: JsonPropertyName("seconds")] int Seconds,
        [property: JsonPropertyName("resource_ids")] IReadOnlyList<string>? ResourceIds);

    private sealed record OptimizationDto(
        [property: JsonPropertyName("optimized_text")] string? OptimizedText,
        [property: JsonPropertyName("rationale")] string? Rationale);

    private sealed record SegmentRefinementDto(
        [property: JsonPropertyName("image_prompt")] string? ImagePrompt,
        [property: JsonPropertyName("video_prompt")] string? VideoPrompt,
        [property: JsonPropertyName("continuity_in")] string? ContinuityIn,
        [property: JsonPropertyName("continuity_out")] string? ContinuityOut,
        [property: JsonPropertyName("shot_plan")] string? ShotPlan,
        [property: JsonPropertyName("rationale")] string? Rationale);
}
