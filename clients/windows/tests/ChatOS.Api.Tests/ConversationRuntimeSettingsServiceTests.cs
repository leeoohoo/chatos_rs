using System.Text.Json;
using ChatOS.Api.Conversation;

namespace ChatOS.Api.Tests;

public sealed class ConversationRuntimeSettingsServiceTests
{
    [Fact]
    public async Task AvailableChatModelsPreserveTaskAvailabilityWithoutFilteringChatOnlyModels()
    {
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            Assert.Equal("/api/chatos/ai-model-configs", request.RequestUri?.AbsolutePath);
            return StubHttpMessageHandler.Json("""
                [
                  {"id":"task-model","name":"Task","model_name":"gpt-task","enabled":true,"task_enabled":true},
                  {"id":"chat-model","name":"Chat","model_name":"gpt-chat","enabled":true,"task_enabled":false}
                ]
                """);
        });
        var service = new ConversationRuntimeSettingsService(client);

        var models = await service.FetchAvailableModelsAsync();

        Assert.Equal(2, models.Count);
        Assert.True(models.Single(model => model.Id == "task-model").TaskEnabled);
        Assert.False(models.Single(model => model.Id == "chat-model").TaskEnabled);
    }

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
