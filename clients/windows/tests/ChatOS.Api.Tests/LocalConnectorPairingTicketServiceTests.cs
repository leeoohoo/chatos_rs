using ChatOS.Api.Authentication;
using ChatOS.Api.Http;

namespace ChatOS.Api.Tests;

public sealed class LocalConnectorPairingTicketServiceTests
{
    [Fact]
    public async Task IssueUsesAuthenticatedPairingTicketEndpoint()
    {
        var store = new MemoryTokenStore();
        store.Seed("api-token");
        var client = ApiTestClient.Create(store, request =>
        {
            Assert.Equal(HttpMethod.Post, request.Method);
            Assert.Equal("/api/chatos/auth/local-connector-ticket", request.RequestUri?.AbsolutePath);
            Assert.Equal("Bearer", request.Headers.Authorization?.Scheme);
            Assert.Equal("api-token", request.Headers.Authorization?.Parameter);
            return StubHttpMessageHandler.Json("{\"ticket\":\"pairing-ticket\"}");
        });

        var ticket = await new LocalConnectorPairingTicketService(client).IssueAsync();

        Assert.Equal("pairing-ticket", ticket);
    }

    [Fact]
    public async Task IssueRejectsEmptyTicketResponse()
    {
        var client = ApiTestClient.Create(new MemoryTokenStore(), _ =>
            StubHttpMessageHandler.Json("{\"ticket\":\"  \"}"));

        var error = await Assert.ThrowsAsync<ChatOSApiException>(() =>
            new LocalConnectorPairingTicketService(client).IssueAsync());

        Assert.Contains("ticket", error.Message, StringComparison.OrdinalIgnoreCase);
    }
}
