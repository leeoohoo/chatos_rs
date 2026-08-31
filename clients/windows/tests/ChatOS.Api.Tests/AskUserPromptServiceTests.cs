using System.Text.Json;
using ChatOS.Api.AskUser;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Tests;

public sealed class AskUserPromptServiceTests
{
    [Fact]
    public async Task FetchMapsFieldsChoicesAndSecretInput()
    {
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            Assert.Equal("conversation_id=c%2F1&include_pending=true&limit=500", request.RequestUri?.Query.TrimStart('?'));
            return StubHttpMessageHandler.Json("""
                {
                  "prompts":[{
                    "id":"prompt-1",
                    "conversation_id":"c/1",
                    "conversation_turn_id":"turn-1",
                    "tool_call_id":"tool-1",
                    "kind":"form",
                    "status":"pending",
                    "created_at":"2026-08-30T10:00:00Z",
                    "prompt":{
                      "title":"需要补充信息",
                      "message":"请填写部署信息",
                      "allow_cancel":true,
                      "timeout_ms":30000,
                      "payload":{
                        "fields":[{
                          "label":"API Key",
                          "description":"仅用于本次执行",
                          "required":true,
                          "multiline":false,
                          "secret":true
                        }],
                        "choice":{
                          "multiple":true,
                          "options":[
                            {"value":"staging","label":"预发布"},
                            {"value":"production","label":"生产"}
                          ],
                          "default":["staging"],
                          "min_selections":1,
                          "max_selections":2
                        }
                      }
                    }
                  }]
                }
                """);
        });
        var service = new AskUserPromptService(client);

        var prompt = Assert.Single(await service.FetchPromptsAsync("c/1", 900));

        Assert.True(prompt.IsPending);
        Assert.Equal("需要补充信息", prompt.Title);
        var field = Assert.Single(prompt.Fields);
        Assert.Equal("api_key", field.Key);
        Assert.True(field.IsSecret);
        Assert.True(field.IsRequired);
        Assert.True(prompt.Choice?.AllowsMultiple);
        Assert.Equal(2, prompt.Choice?.MaximumSelectionCount);
    }

    [Fact]
    public async Task SubmitSerializesMultipleSelectionAndReturnsUpdatedPrompt()
    {
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            Assert.Equal("/api/chatos/ask-user-prompts/prompt%2F1/submit", request.RequestUri?.AbsolutePath);
            var payload = request.Content!.ReadAsStringAsync().GetAwaiter().GetResult();
            using var document = JsonDocument.Parse(payload);
            Assert.Equal("c1", document.RootElement.GetProperty("conversation_id").GetString());
            Assert.Equal("secret", document.RootElement.GetProperty("values").GetProperty("token").GetString());
            Assert.Equal(2, document.RootElement.GetProperty("selection").GetArrayLength());
            return StubHttpMessageHandler.Json(MutationResponse("ok"));
        });
        var service = new AskUserPromptService(client);

        var result = await service.SubmitAsync(
            "prompt/1",
            "c1",
            new AskUserSubmission(
                new Dictionary<string, string> { ["token"] = "secret" },
                new AskUserSelection.Multiple(new[] { "a", "b" })));

        Assert.Equal(AskUserPromptStatus.Ok, result.Status);
    }

    [Fact]
    public async Task CancelUsesUserCancelledReason()
    {
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            var payload = request.Content!.ReadAsStringAsync().GetAwaiter().GetResult();
            Assert.Contains("user_cancelled", payload, StringComparison.Ordinal);
            return StubHttpMessageHandler.Json(MutationResponse("canceled"));
        });
        var service = new AskUserPromptService(client);

        var result = await service.CancelAsync("prompt-1", "c1");

        Assert.Equal(AskUserPromptStatus.Canceled, result.Status);
    }

    private static string MutationResponse(string status) => """
        {
          "prompt":{
            "id":"prompt-1",
            "conversation_id":"c1",
            "conversation_turn_id":"turn-1",
            "kind":"form",
            "status":"__STATUS__",
            "prompt":{"title":"Title","message":"Message","payload":{}}
          }
        }
        """.Replace("__STATUS__", status, StringComparison.Ordinal);
}
