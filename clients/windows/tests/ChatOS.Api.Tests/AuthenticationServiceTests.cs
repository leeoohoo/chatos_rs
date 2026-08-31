using System.Net;
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
            Assert.Equal("/api/chatos/auth/login", request.RequestUri?.AbsolutePath);
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
}
