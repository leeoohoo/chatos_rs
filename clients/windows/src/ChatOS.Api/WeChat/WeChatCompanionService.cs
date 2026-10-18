using System.Text.Json.Serialization;
using ChatOS.Api.Http;

namespace ChatOS.Api.WeChat;

public sealed record WeChatBindingStatus(
    [property: JsonPropertyName("bound")] bool Bound,
    [property: JsonPropertyName("created_at")] string? CreatedAt,
    [property: JsonPropertyName("last_login_at")] string? LastLoginAt);

public sealed record WeChatBindTicket(
    string Id,
    string Scene,
    long ExpiresAtUnix,
    byte[] CodeImageData);

public sealed record WeChatBindTicketStatus(
    [property: JsonPropertyName("ticket_id")] string TicketId,
    [property: JsonPropertyName("status")] string Status,
    [property: JsonPropertyName("expires_at_unix")] long ExpiresAtUnix,
    [property: JsonPropertyName("claimed_at")] string? ClaimedAt);

public sealed record WeChatClientSession(
    [property: JsonPropertyName("id")] string Id,
    [property: JsonPropertyName("client_type")] string ClientType,
    [property: JsonPropertyName("created_at")] string CreatedAt,
    [property: JsonPropertyName("last_seen_at")] string LastSeenAt,
    [property: JsonPropertyName("expires_at_unix")] long ExpiresAtUnix,
    [property: JsonPropertyName("revoked_at")] string? RevokedAt);

public interface IWeChatCompanionService
{
    Task<WeChatBindingStatus> GetBindingStatusAsync(CancellationToken cancellationToken = default);
    Task<WeChatBindTicket> IssueBindTicketAsync(CancellationToken cancellationToken = default);
    Task<WeChatBindTicketStatus> GetBindTicketStatusAsync(
        string id,
        CancellationToken cancellationToken = default);
    Task ConfirmBindTicketAsync(string id, CancellationToken cancellationToken = default);
    Task<IReadOnlyList<WeChatClientSession>> ListClientSessionsAsync(
        CancellationToken cancellationToken = default);
    Task RevokeClientSessionAsync(string id, CancellationToken cancellationToken = default);
    Task UnbindAsync(CancellationToken cancellationToken = default);
}

public sealed class WeChatCompanionService(ChatOSApiClient client) : IWeChatCompanionService
{
    private const int MaximumCodeImageBytes = 2 * 1024 * 1024;

    public Task<WeChatBindingStatus> GetBindingStatusAsync(
        CancellationToken cancellationToken = default) =>
        client.GetUserServiceAsync<WeChatBindingStatus>(
            "auth/wechat/mini-program/binding",
            cancellationToken);

    public async Task<WeChatBindTicket> IssueBindTicketAsync(
        CancellationToken cancellationToken = default)
    {
        var response = await client.PostUserServiceAsync<IssueBindTicketResponse>(
            "auth/wechat/mini-program/bind-tickets",
            cancellationToken: cancellationToken).ConfigureAwait(false);
        var image = DecodeImage(response.QrCodeDataUrl);
        return new WeChatBindTicket(
            response.TicketId,
            response.Scene,
            response.ExpiresAtUnix,
            image);
    }

    public Task<WeChatBindTicketStatus> GetBindTicketStatusAsync(
        string id,
        CancellationToken cancellationToken = default) =>
        client.GetUserServiceAsync<WeChatBindTicketStatus>(
            $"auth/wechat/mini-program/bind-tickets/{Path(id)}",
            cancellationToken);

    public async Task ConfirmBindTicketAsync(
        string id,
        CancellationToken cancellationToken = default) =>
        _ = await client.PostUserServiceAsync<string>(
            $"auth/wechat/mini-program/bind-tickets/{Path(id)}/confirm",
            cancellationToken: cancellationToken).ConfigureAwait(false);

    public async Task<IReadOnlyList<WeChatClientSession>> ListClientSessionsAsync(
        CancellationToken cancellationToken = default) =>
        await client.GetUserServiceAsync<WeChatClientSession[]>(
            "auth/client-sessions",
            cancellationToken).ConfigureAwait(false);

    public async Task RevokeClientSessionAsync(
        string id,
        CancellationToken cancellationToken = default) =>
        _ = await client.DeleteUserServiceAsync(
            $"auth/client-sessions/{Path(id)}",
            cancellationToken).ConfigureAwait(false);

    public async Task UnbindAsync(CancellationToken cancellationToken = default) =>
        _ = await client.DeleteUserServiceAsync(
            "auth/wechat/mini-program/binding",
            cancellationToken).ConfigureAwait(false);

    private static byte[] DecodeImage(string value)
    {
        var comma = value.IndexOf(',');
        if (comma <= 0 ||
            !value[..comma].Contains(";base64", StringComparison.OrdinalIgnoreCase))
        {
            throw new ChatOSApiException("The server did not return a valid Mini Program code.");
        }

        try
        {
            var image = Convert.FromBase64String(value[(comma + 1)..]);
            if (image.Length is 0 or > MaximumCodeImageBytes)
            {
                throw new FormatException();
            }
            return image;
        }
        catch (FormatException exception)
        {
            throw new ChatOSApiException(
                "The server did not return a valid Mini Program code.",
                innerException: exception);
        }
    }

    private static string Path(string value)
    {
        value = value.Trim();
        if (value.Length == 0) throw new ArgumentException("A WeChat resource id is required.");
        return Uri.EscapeDataString(value);
    }

    private sealed record IssueBindTicketResponse(
        [property: JsonPropertyName("ticket_id")] string TicketId,
        [property: JsonPropertyName("scene")] string Scene,
        [property: JsonPropertyName("expires_at_unix")] long ExpiresAtUnix,
        [property: JsonPropertyName("qr_code_data_url")] string QrCodeDataUrl);
}
