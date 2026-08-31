using System.Text.Json;
using ChatOS.Api.Conversation;

namespace ChatOS.Api.Tests;

public sealed class ConversationRuntimeSettingsServiceTests
{
    [Fact]
    public async Task UpdatePlanModeUsesPutAndMapsResponse()
    {
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            Assert.Equal(HttpMethod.Put, request.Method);
            Assert.Equal("/api/chatos/conversations/c1/runtime-settings", request.RequestUri?.AbsolutePath);
            var body = request.Content!.ReadAsStringAsync().GetAwaiter().GetResult();
            using var document = JsonDocument.Parse(body);
            Assert.True(document.RootElement.GetProperty("plan_mode_enabled").GetBoolean());
            return StubHttpMessageHandler.Json("""
                {"selected_model_id":"m1","selected_model_name":"Model","reasoning_enabled":false,"plan_mode_enabled":true}
                """);
        });
        var service = new ConversationRuntimeSettingsService(client);

        var settings = await service.UpdatePlanModeAsync("c1", true);

        Assert.True(settings.PlanModeEnabled);
        Assert.Equal("m1", settings.SelectedModelId);
    }
}
