using System.Net;
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

    [Fact]
    public async Task UnauthorizedResponseFromPreviousSessionDoesNotClearNewSession()
    {
        var store = new MemoryTokenStore();
        store.Seed("token-old");
        var handler = new BlockingHandler();
        var client = new ChatOSApiClient(new HttpClient(handler)
        {
            BaseAddress = new Uri("http://127.0.0.1:9080/"),
        }, store);

        var previousRequest = client.GetUserServiceAsync<object>("auth/me");
        await handler.Started.Task.WaitAsync(TimeSpan.FromSeconds(5));
        await client.SetAccessTokenAsync("token-new");
        handler.Release.TrySetResult();

        var exception = await Assert.ThrowsAsync<ChatOSApiException>(() => previousRequest);
        Assert.Equal(HttpStatusCode.Unauthorized, exception.StatusCode);
        Assert.Equal("token-new", store.Token);
        Assert.Equal(0, store.ClearCount);
    }

    [Fact]
    public async Task UnauthorizedCurrentSessionRaisesExpiryNotification()
    {
        var store = new MemoryTokenStore();
        store.Seed("token-current");
        var handler = new StubHttpMessageHandler(_ => StubHttpMessageHandler.Json(
            "{\"error\":\"invalid or expired token\"}",
            HttpStatusCode.Unauthorized));
        var client = new ChatOSApiClient(new HttpClient(handler)
        {
            BaseAddress = new Uri("http://127.0.0.1:9080/"),
        }, store);
        var expired = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        EventHandler callback = (_, _) => expired.TrySetResult();
        ChatOSApiClient.AuthenticationExpired += callback;
        try
        {
            await Assert.ThrowsAsync<ChatOSApiException>(() => client.GetUserServiceAsync<object>("auth/me"));
            await expired.Task.WaitAsync(TimeSpan.FromSeconds(5));
            Assert.Null(store.Token);
            Assert.Equal(1, store.ClearCount);
        }
        finally
        {
            ChatOSApiClient.AuthenticationExpired -= callback;
        }
    }

    private sealed class BlockingHandler : HttpMessageHandler
    {
        public TaskCompletionSource Started { get; } = new(
            TaskCreationOptions.RunContinuationsAsynchronously);

        public TaskCompletionSource Release { get; } = new(
            TaskCreationOptions.RunContinuationsAsynchronously);

        protected override async Task<HttpResponseMessage> SendAsync(
            HttpRequestMessage request,
            CancellationToken cancellationToken)
        {
            Started.TrySetResult();
            await Release.Task.WaitAsync(cancellationToken);
            return StubHttpMessageHandler.Json(
                "{\"error\":\"invalid or expired token\"}",
                HttpStatusCode.Unauthorized);
        }
    }
}
