using ChatOS.Api.Http;
using ChatOS.Api.Realtime;
using Microsoft.Extensions.Options;

namespace ChatOS.Api.Tests;

public sealed class ChatOSRealtimeClientTests
{
    [Fact]
    public void BuildsEncodedWebSocketUriFromApiBaseUrl()
    {
        var store = new MemoryTokenStore();
        var apiClient = ApiTestClient.Create(store, _ => StubHttpMessageHandler.Json("{}"));
        var client = new ChatOSRealtimeClient(
            new WebSocketTicketService(apiClient),
            Options.Create(new ChatOSApiOptions
            {
                BaseUrl = "https://example.test/api/chatos",
            }));

        var uri = client.BuildWebSocketUri("ticket +/=");

        Assert.Equal("wss", uri.Scheme);
        Assert.Equal("/api/chatos/realtime/ws", uri.AbsolutePath);
        Assert.Equal("?ws_ticket=ticket%20%2B%2F%3D", uri.Query);
    }

    [Fact]
    public void SubscriptionPayloadsUseGatewayTopicContract()
    {
        Assert.Equal(
            "{\"type\":\"subscribe\",\"topics\":[{\"scope\":\"conversation\",\"id\":\"c1\"}]}",
            ChatOSRealtimeClient.ConversationSubscription("c1"));
        Assert.Equal(
            "{\"type\":\"subscribe\",\"topics\":[{\"scope\":\"user\"}]}",
            ChatOSRealtimeClient.UserSubscription());
    }
}
