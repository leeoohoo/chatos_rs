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
    private readonly SemaphoreSlim _authenticationGate = new(1, 1);
    private long _authenticationGeneration;

    /// <summary>
    /// Raised when a request made with the current user session receives 401 and the
    /// same session is still active when the response is processed. The desktop shell
    /// uses this to mirror macOS' authentication-expired notification and clear all
    /// account-scoped UI state.
    /// </summary>
    public static event EventHandler? AuthenticationExpired;

    public ChatOSApiClient(HttpClient httpClient, IAuthTokenStore tokenStore)
    {
        _httpClient = httpClient;
        _tokenStore = tokenStore;
    }

    public async ValueTask SetAccessTokenAsync(
        string? token,
        CancellationToken cancellationToken = default)
    {
        await _authenticationGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            _authenticationGeneration++;
            token = token?.Trim();
            if (string.IsNullOrEmpty(token))
            {
                await _tokenStore.ClearAsync(cancellationToken).ConfigureAwait(false);
            }
            else
            {
                await _tokenStore.SetAccessTokenAsync(token, cancellationToken).ConfigureAwait(false);
            }
        }
        finally
        {
            _authenticationGate.Release();
        }
    }

    public Task<T> GetUserServiceAsync<T>(
        string path,
        CancellationToken cancellationToken = default) =>
        SendCoreAsync<T>(
            HttpMethod.Get,
            path,
            null,
            cancellationToken);

    public Task<T> PostUserServiceAsync<T>(
        string path,
        object? body = null,
        CancellationToken cancellationToken = default) =>
        SendCoreAsync<T>(
            HttpMethod.Post,
            path,
            body,
            cancellationToken);

    public Task<string> DeleteUserServiceAsync(
        string path,
        CancellationToken cancellationToken = default) =>
        SendCoreAsync<string>(
            HttpMethod.Delete,
            path,
            null,
            cancellationToken);

    private async Task<T> SendCoreAsync<T>(
        HttpMethod method,
        string path,
        object? body,
        CancellationToken cancellationToken = default)
    {
        using var timeoutSource = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeoutSource.CancelAfter(DefaultRequestTimeout);
        var requestCancellationToken = timeoutSource.Token;
        using var request = new HttpRequestMessage(method, ResolveUserServiceUri(path));
        try
        {
            var authentication = await CaptureAuthenticationAsync(requestCancellationToken).ConfigureAwait(false);
            var token = authentication.Token;
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
                var sessionExpired = await ClearAccessTokenIfCurrentAsync(
                    authentication,
                    requestCancellationToken).ConfigureAwait(false);
                if (sessionExpired)
                {
                    AuthenticationExpired?.Invoke(null, EventArgs.Empty);
                }
            }
            else if (TryGetRefreshedAccessToken(response, out var refreshedToken))
            {
                await RefreshAccessTokenIfCurrentAsync(
                    authentication,
                    refreshedToken,
                    requestCancellationToken).ConfigureAwait(false);
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

    private async ValueTask<AuthenticationSnapshot> CaptureAuthenticationAsync(
        CancellationToken cancellationToken)
    {
        await _authenticationGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            var token = (await _tokenStore.GetAccessTokenAsync(cancellationToken)
                .ConfigureAwait(false))?.Trim();
            return new AuthenticationSnapshot(
                string.IsNullOrEmpty(token) ? null : token,
                _authenticationGeneration);
        }
        finally
        {
            _authenticationGate.Release();
        }
    }

    private async ValueTask<bool> ClearAccessTokenIfCurrentAsync(
        AuthenticationSnapshot requestAuthentication,
        CancellationToken cancellationToken)
    {
        await _authenticationGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            var currentToken = (await _tokenStore.GetAccessTokenAsync(cancellationToken)
                .ConfigureAwait(false))?.Trim();
            if (_authenticationGeneration != requestAuthentication.Generation ||
                !string.Equals(currentToken, requestAuthentication.Token, StringComparison.Ordinal))
            {
                return false;
            }

            _authenticationGeneration++;
            await _tokenStore.ClearAsync(cancellationToken).ConfigureAwait(false);
            return true;
        }
        finally
        {
            _authenticationGate.Release();
        }
    }

    private async ValueTask RefreshAccessTokenIfCurrentAsync(
        AuthenticationSnapshot requestAuthentication,
        string refreshedToken,
        CancellationToken cancellationToken)
    {
        await _authenticationGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            var currentToken = (await _tokenStore.GetAccessTokenAsync(cancellationToken)
                .ConfigureAwait(false))?.Trim();
            if (_authenticationGeneration != requestAuthentication.Generation ||
                !string.Equals(currentToken, requestAuthentication.Token, StringComparison.Ordinal))
            {
                return;
            }

            await _tokenStore.SetAccessTokenAsync(refreshedToken, cancellationToken)
                .ConfigureAwait(false);
        }
        finally
        {
            _authenticationGate.Release();
        }
    }

    private static bool TryGetRefreshedAccessToken(
        HttpResponseMessage response,
        out string refreshedToken)
    {
        refreshedToken = string.Empty;
        if (!response.Headers.TryGetValues("X-Access-Token", out var values))
        {
            return false;
        }

        refreshedToken = values.FirstOrDefault()?.Trim() ?? string.Empty;
        return refreshedToken.Length > 0;
    }

    private readonly record struct AuthenticationSnapshot(string? Token, long Generation);

    private static string NormalizePath(string path) => path.TrimStart('/');

    private Uri ResolveUserServiceUri(string path)
    {
        var normalizedPath = NormalizePath(path);
        var baseAddress = _httpClient.BaseAddress
            ?? throw new InvalidOperationException("ChatOS API base address is required.");
        if (!string.IsNullOrEmpty(baseAddress.UserInfo) ||
            !string.IsNullOrEmpty(baseAddress.Query) ||
            !string.IsNullOrEmpty(baseAddress.Fragment))
        {
            throw new InvalidOperationException(
                "ChatOS API base address must not contain credentials, a query, or a fragment.");
        }

        var basePath = baseAddress.AbsolutePath.TrimEnd('/');
        var builder = new UriBuilder(baseAddress)
        {
            Path = $"{basePath}/api/user/",
            Query = string.Empty,
            Fragment = string.Empty,
        };
        return new Uri(builder.Uri, normalizedPath);
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
