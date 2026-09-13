using ChatOS.Api.Pet;

namespace ChatOS.Api.Tests;

public sealed class PetConversationControlServiceTests
{
    [Fact]
    public async Task StopTurnSendsOnlyThePetActivityConversationIdentity()
    {
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            Assert.Equal("/api/chatos/agent/chat/stop", request.RequestUri?.AbsolutePath);
            var body = request.Content!.ReadAsStringAsync().GetAwaiter().GetResult();
            Assert.Contains("\"conversation_id\":\"c1\"", body, StringComparison.Ordinal);
            Assert.Contains("\"turn_id\":\"turn-1\"", body, StringComparison.Ordinal);
            return StubHttpMessageHandler.Json("{\"success\":true}");
        });

        await new PetConversationControlService(client).StopTurnAsync("c1", "turn-1");
    }
}
