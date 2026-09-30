using ChatOS.Api.Http;

namespace ChatOS.Api.Tests;

public sealed class ChatOSApiClientTests
{
    [Fact]
    public async Task UserServiceRequestPreservesGatewayPrefix()
    {
        var handler = new StubHttpMessageHandler(request =>
        {
            Assert.Equal("/prefix/api/user/health", request.RequestUri?.AbsolutePath);
            return StubHttpMessageHandler.Json("{}");
        });
        var httpClient = new HttpClient(handler)
        {
            BaseAddress = new Uri("https://gateway.example/prefix/"),
        };
        var client = new ChatOSApiClient(httpClient, new MemoryTokenStore());

        await client.GetUserServiceAsync<string>("health");
    }

    [Fact]
    public async Task InvalidGatewayRootIsRejectedBeforeTransport()
    {
        var called = false;
        var handler = new StubHttpMessageHandler(_ =>
        {
            called = true;
            return StubHttpMessageHandler.Json("{}");
        });
        var httpClient = new HttpClient(handler)
        {
            BaseAddress = new Uri("https://user:password@gateway.example/"),
        };
        var client = new ChatOSApiClient(httpClient, new MemoryTokenStore());

        await Assert.ThrowsAsync<InvalidOperationException>(
            () => client.GetUserServiceAsync<string>("health"));
        Assert.False(called);
    }
}
