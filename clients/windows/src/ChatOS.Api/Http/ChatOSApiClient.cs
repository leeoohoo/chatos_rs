using System.Net;
using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Text.Json;

namespace ChatOS.Api.Http;

public sealed class ChatOSApiClient
{
    private static readonly TimeSpan DefaultRequestTimeout = TimeSpan.FromSeconds(60);
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = true,
    };

    private readonly HttpClient _httpClient;
    private readonly IAuthTokenStore _tokenStore;

    public ChatOSApiClient(HttpClient httpClient, IAuthTokenStore tokenStore)
    {
        _httpClient = httpClient;
        _tokenStore = tokenStore;
    }

    public Task<T> GetAsync<T>(string path, CancellationToken cancellationToken = default) =>
        SendAsync<T>(HttpMethod.Get, path, null, cancellationToken);

    public Task<T> GetTaskRunnerAsync<T>(
        string path,
        CancellationToken cancellationToken = default) =>
        SendAsync<T>(HttpMethod.Get, TaskRunnerPath(path), null, cancellationToken);

    public Task<T> PostAsync<T>(
        string path,
        object? body = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<T>(HttpMethod.Post, path, body, cancellationToken);

    public Task<T> PostTaskRunnerAsync<T>(
        string path,
        object? body = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<T>(HttpMethod.Post, TaskRunnerPath(path), body, cancellationToken);

    public Task<T> PostAsync<T>(
        string path,
        object? body,
        TimeSpan timeout,
        CancellationToken cancellationToken = default) =>
        SendAsync<T>(HttpMethod.Post, path, body, cancellationToken, timeout);

    public Task<T> PutAsync<T>(
        string path,
        object? body = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<T>(HttpMethod.Put, path, body, cancellationToken);

    public Task<T> DeleteAsync<T>(
        string path,
        CancellationToken cancellationToken = default) =>
        SendAsync<T>(HttpMethod.Delete, path, null, cancellationToken);

    public async Task<T> SendAsync<T>(
        HttpMethod method,
        string path,
        object? body,
        CancellationToken cancellationToken = default,
        TimeSpan? timeout = null)
    {
        using var timeoutSource = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeoutSource.CancelAfter(timeout ?? DefaultRequestTimeout);
        var requestCancellationToken = timeoutSource.Token;
        using var request = new HttpRequestMessage(method, NormalizePath(path));
        try
        {
            var token = await _tokenStore.GetAccessTokenAsync(requestCancellationToken).ConfigureAwait(false);
            if (!string.IsNullOrWhiteSpace(token))
            {
                request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", token);
            }

            request.Headers.TryAddWithoutValidation("X-ChatOS-Client", "windows-native");
            request.Headers.TryAddWithoutValidation("X-Correlation-ID", Guid.NewGuid().ToString("N"));
            if (body is not null)
            {
                request.Content = JsonContent.Create(body, body.GetType(), options: JsonOptions);
            }

            using var response = await _httpClient.SendAsync(
                request,
                HttpCompletionOption.ResponseHeadersRead,
                requestCancellationToken).ConfigureAwait(false);
            var payload = await response.Content
                .ReadAsStringAsync(requestCancellationToken)
                .ConfigureAwait(false);
            if (response.StatusCode == HttpStatusCode.Unauthorized)
            {
                await _tokenStore.ClearAsync(requestCancellationToken).ConfigureAwait(false);
            }

            if (!response.IsSuccessStatusCode)
            {
                throw new ChatOSApiException(
                    ResolveErrorMessage(payload, response.StatusCode),
                    response.StatusCode,
                    payload);
            }

            if (typeof(T) == typeof(string))
            {
                return (T)(object)payload;
            }

            try
            {
                return JsonSerializer.Deserialize<T>(payload, JsonOptions)
                    ?? throw new JsonException("Response body was empty.");
            }
            catch (JsonException exception)
            {
                throw new ChatOSApiException(
                    "The ChatOS gateway returned an invalid response.",
                    response.StatusCode,
                    payload,
                    exception);
            }
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            throw new ChatOSApiException("ChatOS request timed out.");
        }
        catch (HttpRequestException exception)
        {
            throw new ChatOSApiException("Unable to connect to the ChatOS gateway.", innerException: exception);
        }
    }

    private static string NormalizePath(string path) => path.TrimStart('/');

    private string TaskRunnerPath(string path)
    {
        var baseAddress = _httpClient.BaseAddress
            ?? throw new ChatOSApiException("ChatOS API base URL is not configured.");
        var builder = new UriBuilder(baseAddress);
        if (!string.IsNullOrEmpty(builder.Query) || !string.IsNullOrEmpty(builder.Fragment) ||
            !string.IsNullOrEmpty(builder.UserName) || !string.IsNullOrEmpty(builder.Password))
        {
            throw new ChatOSApiException("ChatOS API base URL is invalid for Task Runner routing.");
        }
        var prefix = builder.Path.TrimEnd('/');
        const string chatOsSuffix = "/api/chatos";
        if (prefix.EndsWith(chatOsSuffix, StringComparison.Ordinal))
        {
            prefix = prefix[..^chatOsSuffix.Length];
        }
        else if (prefix.Length > 0)
        {
            throw new ChatOSApiException("ChatOS API base URL cannot be routed to Task Runner.");
        }
        var normalizedPath = path.TrimStart('/');
        var querySeparator = normalizedPath.IndexOf('?');
        var route = querySeparator >= 0
            ? normalizedPath[..querySeparator]
            : normalizedPath;
        var query = querySeparator >= 0
            ? normalizedPath[(querySeparator + 1)..]
            : string.Empty;
        builder.Path = $"{prefix}/api/task/{route}";
        builder.Query = query;
        return builder.Uri.AbsoluteUri;
    }

    private static string ResolveErrorMessage(string payload, HttpStatusCode statusCode)
    {
        if (!string.IsNullOrWhiteSpace(payload))
        {
            try
            {
                using var document = JsonDocument.Parse(payload);
                foreach (var key in new[] { "message", "detail", "error" })
                {
                    if (document.RootElement.TryGetProperty(key, out var value) &&
                        value.ValueKind == JsonValueKind.String &&
                        !string.IsNullOrWhiteSpace(value.GetString()))
                    {
                        return value.GetString()!;
                    }
                }
            }
            catch (JsonException)
            {
                // The status fallback below is safer than exposing an HTML gateway body.
            }
        }

        return statusCode switch
        {
            HttpStatusCode.Unauthorized => "Your ChatOS session has expired.",
            HttpStatusCode.Forbidden => "This operation is not permitted.",
            HttpStatusCode.NotFound => "The requested ChatOS resource was not found.",
            HttpStatusCode.Conflict => "The resource changed before the operation completed.",
            HttpStatusCode.TooManyRequests => "ChatOS is busy. Try again shortly.",
            _ => $"ChatOS request failed with status {(int)statusCode}.",
        };
    }
}
