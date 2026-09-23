using System.Collections.Concurrent;
using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using ChatOS.Connector.Persistence;

namespace ChatOS.Connector.Plugins;
public sealed partial class PluginOAuthBroker
{
    private sealed record PendingTransaction(
        string TransactionId,
        string OwnerUserId,
        string DeviceId,
        InstalledPluginRecord Record,
        string ComponentKey,
        Uri RedirectUri,
        string CodeVerifier,
        OAuthAppManifest App,
        DateTimeOffset ExpiresAt,
        TcpListener Listener);

    private sealed record OAuthAppManifest
    {
        [System.Text.Json.Serialization.JsonPropertyName("schemaVersion")]
        public required int SchemaVersion { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("provider")]
        public required string Provider { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("clientId")]
        public required string ClientId { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("authorizationUrl")]
        public required Uri AuthorizationUrl { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("tokenUrl")]
        public required Uri TokenUrl { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("resource")]
        public required string Resource { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("scopes")]
        public IReadOnlyList<string> Scopes { get; init; } = [];
        [System.Text.Json.Serialization.JsonPropertyName("callbackType")]
        public required string CallbackType { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("authorizationParams")]
        public IReadOnlyDictionary<string, string> AuthorizationParams { get; init; } =
            new Dictionary<string, string>();

        public void Validate()
        {
            if (SchemaVersion != 1 || CallbackType != "loopback" ||
                string.IsNullOrWhiteSpace(Provider) || Provider.Length > 96 ||
                string.IsNullOrWhiteSpace(ClientId) || ClientId.Length > 512 ||
                string.IsNullOrWhiteSpace(Resource) || Resource.Length > 2048)
            {
                throw new PluginRuntimeException("Plugin OAuth app manifest is invalid.");
            }

            ValidateEndpoint(AuthorizationUrl);
            ValidateEndpoint(TokenUrl);
            _ = NormalizeScopes(Scopes);
            var reserved = new HashSet<string>(StringComparer.Ordinal)
            {
                "response_type", "client_id", "redirect_uri", "state", "scope",
                "code_challenge", "code_challenge_method",
            };
            if (AuthorizationParams.Count > 32 || AuthorizationParams.Keys.Any(reserved.Contains) ||
                AuthorizationParams.Any(pair =>
                    string.IsNullOrWhiteSpace(pair.Key) || pair.Key.Length > 96 ||
                    string.IsNullOrWhiteSpace(pair.Value) || pair.Value.Length > 2048))
            {
                throw new PluginRuntimeException("Plugin OAuth authorization parameters are invalid.");
            }
        }

        private static void ValidateEndpoint(Uri uri)
        {
            var loopback = IPAddress.TryParse(uri.Host, out var address) && IPAddress.IsLoopback(address) ||
                uri.Host.Equals("localhost", StringComparison.OrdinalIgnoreCase);
            if ((uri.Scheme != Uri.UriSchemeHttps && !(uri.Scheme == Uri.UriSchemeHttp && loopback)) ||
                !string.IsNullOrEmpty(uri.UserInfo) || !string.IsNullOrEmpty(uri.Fragment))
            {
                throw new PluginRuntimeException("Plugin OAuth endpoints require HTTPS except for loopback development.");
            }
        }
    }

    private sealed record OAuthTokenResponse
    {
        [System.Text.Json.Serialization.JsonPropertyName("access_token")]
        public required string AccessToken { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("refresh_token")]
        public string? RefreshToken { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("expires_in")]
        public long? ExpiresIn { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("scope")]
        public string? Scope { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("token_type")]
        public string? TokenType { get; init; }

        public void Validate()
        {
            if (string.IsNullOrWhiteSpace(AccessToken) || AccessToken.Length > 64 * 1024 ||
                RefreshToken?.Length > 64 * 1024 || ExpiresIn is < 0 or > 315_360_000 ||
                (TokenType is not null && !TokenType.Equals("Bearer", StringComparison.OrdinalIgnoreCase)))
            {
                throw new PluginRuntimeException("Plugin OAuth token response is invalid.");
            }
        }
    }
}
