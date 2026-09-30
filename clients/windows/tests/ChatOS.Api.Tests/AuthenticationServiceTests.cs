using System.Net;
using System.Text.Json;
using ChatOS.Api.Authentication;

namespace ChatOS.Api.Tests;

public sealed class AuthenticationServiceTests
{
    [Fact]
    public async Task LoginPersistsReturnedTokenAndMapsUser()
    {
        var store = new MemoryTokenStore();
        var client = ApiTestClient.Create(store, request =>
        {
            Assert.Equal(HttpMethod.Post, request.Method);
            Assert.Equal("/api/user/auth/login", request.RequestUri?.AbsolutePath);
            return StubHttpMessageHandler.Json("""
                {"access_token":"token-123","user":{"id":"u1","username":"lilei","display_name":"李雷","role":"user"}}
                """);
        });
        var service = new AuthenticationService(client, store);

        var session = await service.LoginAsync("  lilei  ", "secret");

        Assert.Equal("token-123", store.Token);
        Assert.Equal("李雷", session.User.EffectiveDisplayName);
        Assert.Equal("user", session.User.Role);
    }

    [Fact]
    public async Task RestoreReturnsNullAndClearsExpiredToken()
    {
        var store = new MemoryTokenStore();
        store.Seed("expired");
        var client = ApiTestClient.Create(store, _ =>
            StubHttpMessageHandler.Json("{\"detail\":\"expired\"}", HttpStatusCode.Unauthorized));
        var service = new AuthenticationService(client, store);

        var session = await service.RestoreSessionAsync();

        Assert.Null(session);
        Assert.Null(store.Token);
        Assert.Equal(1, store.ClearCount);
    }

    [Fact]
    public async Task RestoreDoesNotCallGatewayWithoutStoredToken()
    {
        var called = false;
        var store = new MemoryTokenStore();
        var client = ApiTestClient.Create(store, _ =>
        {
            called = true;
            return StubHttpMessageHandler.Json("{}");
        });
        var service = new AuthenticationService(client, store);

        Assert.Null(await service.RestoreSessionAsync());
        Assert.False(called);
    }

    [Fact]
    public async Task SendRegistrationCodeTrimsEmailAndInvitationCode()
    {
        var store = new MemoryTokenStore();
        var client = ApiTestClient.Create(store, request =>
        {
            Assert.Equal(HttpMethod.Post, request.Method);
            Assert.Equal("/api/user/auth/register/send-code", request.RequestUri?.AbsolutePath);
            using var body = JsonDocument.Parse(request.Content!.ReadAsStringAsync().GetAwaiter().GetResult());
            Assert.Equal("person@example.com", body.RootElement.GetProperty("email").GetString());
            Assert.Equal("invite-123", body.RootElement.GetProperty("invite_code").GetString());
            return StubHttpMessageHandler.Json(
                "{\"ok\":true,\"expires_in_seconds\":600,\"resend_after_seconds\":60}");
        });
        var service = new AuthenticationService(client, store);

        var delivery = await service.SendRegistrationCodeAsync(
            " person@example.com ", " invite-123 ");

        Assert.Equal(600, delivery.ExpiresInSeconds);
        Assert.Equal(60, delivery.ResendAfterSeconds);
        Assert.Null(store.Token);
    }

    [Fact]
    public async Task RegisterPersistsTokenAndSendsCompleteInvitationPayload()
    {
        var store = new MemoryTokenStore();
        var client = ApiTestClient.Create(store, request =>
        {
            Assert.Equal(HttpMethod.Post, request.Method);
            Assert.Equal("/api/user/auth/register", request.RequestUri?.AbsolutePath);
            using var body = JsonDocument.Parse(request.Content!.ReadAsStringAsync().GetAwaiter().GetResult());
            var root = body.RootElement;
            Assert.Equal("new@example.com", root.GetProperty("email").GetString());
            Assert.Equal("secret-value", root.GetProperty("password").GetString());
            Assert.Equal("invite-123", root.GetProperty("invite_code").GetString());
            Assert.Equal("123456", root.GetProperty("verification_code").GetString());
            return StubHttpMessageHandler.Json("""
                {"access_token":"registered-token","user":{"id":"u-new","username":"new@example.com","display_name":null,"role":"user"}}
                """);
        });
        var service = new AuthenticationService(client, store);

        var session = await service.RegisterAsync(
            " new@example.com ", "secret-value", " invite-123 ", " 123456 ");

        Assert.Equal("registered-token", store.Token);
        Assert.Equal("new@example.com", session.User.Username);
    }
}
