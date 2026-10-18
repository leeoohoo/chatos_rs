using System.Net;
using ChatOS.Api.Http;
using ChatOS.Api.WeChat;

namespace ChatOS.Api.Tests;

public sealed class WeChatCompanionServiceTests
{
    [Fact]
    public async Task Binding_ticket_and_session_endpoints_match_the_macos_contract()
    {
        var calls = new List<(HttpMethod Method, string Path)>();
        var image = Convert.ToBase64String([1, 2, 3, 4]);
        var client = ApiTestClient.Create(new MemoryTokenStore(), request =>
        {
            var path = request.RequestUri!.AbsolutePath;
            calls.Add((request.Method, path));
            return path switch
            {
                "/api/user/auth/wechat/mini-program/binding" when request.Method == HttpMethod.Get =>
                    StubHttpMessageHandler.Json("{\"bound\":false,\"created_at\":null,\"last_login_at\":null}"),
                "/api/user/auth/wechat/mini-program/bind-tickets" =>
                    StubHttpMessageHandler.Json($"{{\"ticket_id\":\"ticket one\",\"scene\":\"scene\",\"expires_at_unix\":2000000000,\"qr_code_data_url\":\"data:image/png;base64,{image}\"}}"),
                "/api/user/auth/wechat/mini-program/bind-tickets/ticket%20one" when request.Method == HttpMethod.Get =>
                    StubHttpMessageHandler.Json("{\"ticket_id\":\"ticket one\",\"status\":\"claimed\",\"expires_at_unix\":2000000000,\"claimed_at\":\"2026-01-01T00:00:00Z\"}"),
                "/api/user/auth/wechat/mini-program/bind-tickets/ticket%20one/confirm" =>
                    StubHttpMessageHandler.Json("{\"status\":\"confirmed\"}"),
                "/api/user/auth/client-sessions" =>
                    StubHttpMessageHandler.Json("[{\"id\":\"session one\",\"client_type\":\"wechat_mini_program\",\"created_at\":\"2026-01-01T00:00:00Z\",\"last_seen_at\":\"2026-01-02T00:00:00Z\",\"expires_at_unix\":2000000000,\"revoked_at\":null}]"),
                "/api/user/auth/client-sessions/session%20one" =>
                    new HttpResponseMessage(HttpStatusCode.NoContent),
                "/api/user/auth/wechat/mini-program/binding" =>
                    new HttpResponseMessage(HttpStatusCode.NoContent),
                _ => StubHttpMessageHandler.Json("{}", HttpStatusCode.NotFound),
            };
        });
        var service = new WeChatCompanionService(client);

        Assert.False((await service.GetBindingStatusAsync()).Bound);
        var ticket = await service.IssueBindTicketAsync();
        Assert.Equal([1, 2, 3, 4], ticket.CodeImageData);
        Assert.Equal("claimed", (await service.GetBindTicketStatusAsync(ticket.Id)).Status);
        await service.ConfirmBindTicketAsync(ticket.Id);
        var session = Assert.Single(await service.ListClientSessionsAsync());
        await service.RevokeClientSessionAsync(session.Id);
        await service.UnbindAsync();

        Assert.Contains((HttpMethod.Post, "/api/user/auth/wechat/mini-program/bind-tickets"), calls);
        Assert.Contains((HttpMethod.Post, "/api/user/auth/wechat/mini-program/bind-tickets/ticket%20one/confirm"), calls);
        Assert.Contains((HttpMethod.Delete, "/api/user/auth/client-sessions/session%20one"), calls);
        Assert.Contains((HttpMethod.Delete, "/api/user/auth/wechat/mini-program/binding"), calls);
    }

    [Fact]
    public async Task Invalid_or_oversized_code_images_are_rejected()
    {
        var invalidClient = ApiTestClient.Create(new MemoryTokenStore(), _ =>
            StubHttpMessageHandler.Json("{\"ticket_id\":\"ticket\",\"scene\":\"scene\",\"expires_at_unix\":2000000000,\"qr_code_data_url\":\"not-a-data-url\"}"));

        var error = await Assert.ThrowsAsync<ChatOSApiException>(
            () => new WeChatCompanionService(invalidClient).IssueBindTicketAsync());

        Assert.Contains("valid Mini Program code", error.Message, StringComparison.Ordinal);
    }
}
