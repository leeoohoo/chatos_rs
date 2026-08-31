using System.Net;
using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Authentication;

public sealed class AuthenticationService : IAuthenticationService
{
    private readonly ChatOSApiClient _client;
    private readonly IAuthTokenStore _tokenStore;

    public AuthenticationService(ChatOSApiClient client, IAuthTokenStore tokenStore)
    {
        _client = client;
        _tokenStore = tokenStore;
    }

    public async Task<AuthSession?> RestoreSessionAsync(
        CancellationToken cancellationToken = default)
    {
        var token = (await _tokenStore.GetAccessTokenAsync(cancellationToken)
            .ConfigureAwait(false))?.Trim();
        if (string.IsNullOrEmpty(token))
        {
            return null;
        }

        try
        {
            var response = await _client.GetAsync<MeResponseDto>(
                "auth/me",
                cancellationToken).ConfigureAwait(false);
            return new AuthSession(response.User.ToDomain());
        }
        catch (ChatOSApiException exception) when (exception.StatusCode == HttpStatusCode.Unauthorized)
        {
            return null;
        }
    }

    public async Task<AuthSession> LoginAsync(
        string username,
        string password,
        CancellationToken cancellationToken = default)
    {
        username = username.Trim();
        if (username.Length == 0 || password.Length == 0)
        {
            throw new ArgumentException("Username and password are required.");
        }

        var response = await _client.PostAsync<LoginResponseDto>(
            "auth/login",
            new LoginRequestDto(username, password),
            cancellationToken).ConfigureAwait(false);
        var token = response.AccessToken.Trim();
        if (token.Length == 0)
        {
            throw new ChatOSApiException("The login response did not include an access token.");
        }

        await _tokenStore.SetAccessTokenAsync(token, cancellationToken).ConfigureAwait(false);
        return new AuthSession(response.User.ToDomain());
    }

    public ValueTask LogoutAsync(CancellationToken cancellationToken = default) =>
        _tokenStore.ClearAsync(cancellationToken);
}

internal sealed record LoginRequestDto(
    [property: JsonPropertyName("username")] string Username,
    [property: JsonPropertyName("password")] string Password);

internal sealed record LoginResponseDto(
    [property: JsonPropertyName("access_token")] string AccessToken,
    [property: JsonPropertyName("user")] AuthUserDto User);

internal sealed record MeResponseDto(
    [property: JsonPropertyName("user")] AuthUserDto User);

internal sealed record AuthUserDto(
    [property: JsonPropertyName("id")] string Id,
    [property: JsonPropertyName("username")] string Username,
    [property: JsonPropertyName("display_name")] string? DisplayName,
    [property: JsonPropertyName("role")] string Role)
{
    public AuthUser ToDomain() => new(Id, Username, DisplayName, Role);
}
