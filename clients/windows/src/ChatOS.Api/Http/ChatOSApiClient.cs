using System.Net;
using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Text.Json;

namespace ChatOS.Api.Http;

public sealed class ChatOSApiClient
{
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

    public Task<T> PostAsync<T>(
        string path,
        object? body = null,
        CancellationToken cancellationToken = default) =>
        SendAsync<T>(HttpMethod.Post, path, body, cancellationToken);

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
        CancellationToken cancellationToken = default)
    {
        using var request = new HttpRequestMessage(method, NormalizePath(path));
        var token = await _tokenStore.GetAccessTokenAsync(cancellationToken).ConfigureAwait(false);
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

        HttpResponseMessage response;
        try
        {
            response = await _httpClient.SendAsync(
                request,
                HttpCompletionOption.ResponseHeadersRead,
                cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            throw new ChatOSApiException("ChatOS request timed out.");
        }
        catch (HttpRequestException exception)
        {
            throw new ChatOSApiException("Unable to connect to the ChatOS gateway.", innerException: exception);
        }

        using (response)
        {
            var payload = await response.Content.ReadAsStringAsync(cancellationToken).ConfigureAwait(false);
            if (response.StatusCode == HttpStatusCode.Unauthorized)
            {
                await _tokenStore.ClearAsync(cancellationToken).ConfigureAwait(false);
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
    }

    private static string NormalizePath(string path) => path.TrimStart('/');

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
