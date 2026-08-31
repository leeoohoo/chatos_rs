using System.Net;
using System.Text;
using ChatOS.Api.Http;

namespace ChatOS.Api.Tests;

internal sealed class MemoryTokenStore : IAuthTokenStore
{
    public string? Token { get; private set; }

    public int ClearCount { get; private set; }

    public ValueTask<string?> GetAccessTokenAsync(CancellationToken cancellationToken = default) =>
        ValueTask.FromResult(Token);

    public ValueTask SetAccessTokenAsync(string token, CancellationToken cancellationToken = default)
    {
        Token = token;
        return ValueTask.CompletedTask;
    }

    public ValueTask ClearAsync(CancellationToken cancellationToken = default)
    {
        Token = null;
        ClearCount++;
        return ValueTask.CompletedTask;
    }

    public void Seed(string token) => Token = token;
}

internal sealed class StubHttpMessageHandler : HttpMessageHandler
{
    private readonly Func<HttpRequestMessage, HttpResponseMessage> _handler;

    public StubHttpMessageHandler(Func<HttpRequestMessage, HttpResponseMessage> handler)
    {
        _handler = handler;
    }

    protected override Task<HttpResponseMessage> SendAsync(
        HttpRequestMessage request,
        CancellationToken cancellationToken) => Task.FromResult(_handler(request));

    public static HttpResponseMessage Json(string json, HttpStatusCode status = HttpStatusCode.OK) =>
        new(status)
        {
            Content = new StringContent(json, Encoding.UTF8, "application/json"),
        };
}

internal static class ApiTestClient
{
    public static ChatOSApiClient Create(
        MemoryTokenStore tokenStore,
        Func<HttpRequestMessage, HttpResponseMessage> handler)
    {
        var httpClient = new HttpClient(new StubHttpMessageHandler(handler))
        {
            BaseAddress = new Uri("http://127.0.0.1:9080/api/chatos/"),
        };
        return new ChatOSApiClient(httpClient, tokenStore);
    }
}
