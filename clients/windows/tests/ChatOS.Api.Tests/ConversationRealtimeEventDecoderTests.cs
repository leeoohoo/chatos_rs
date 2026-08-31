using ChatOS.Api.Realtime;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Tests;

public sealed class ConversationRealtimeEventDecoderTests
{
    [Fact]
    public void AskUserEventMapsPromptIdentityAndStatus()
    {
        const string json = """
        {
          "type":"event",
          "event":"ask_user.updated",
          "event_id":"e1",
          "event_sequence":7,
          "conversation_id":"c1",
          "ts":"2026-08-30T10:00:00Z",
          "payload":{
            "kind":"ask_user_prompt",
            "prompt_id":"prompt-1",
            "conversation_turn_id":"turn-1",
            "action":"answered",
            "status":"resolved"
          }
        }
        """;

        var signal = ConversationRealtimeEventDecoder.Decode(json, "c1");

        Assert.NotNull(signal);
        Assert.Equal("prompt-1", signal.AskUserPromptUpdate?.PromptId);
        Assert.Equal("resolved", signal.AskUserPromptUpdate?.Status);
    }

    [Fact]
    public void ToolStartEventMapsVisibleProcessDetail()
    {
        const string json = """
        {
          "type":"event",
          "event":"tool.started",
          "event_id":"e2",
          "event_sequence":8,
          "conversation_id":"c1",
          "ts":"2026-08-30T10:00:01Z",
          "payload":{
            "kind":"chat_stream",
            "conversation_turn_id":"turn-1",
            "raw":{"type":"tools_start","data":{"tool_calls":[{"name":"read_file"},{"function":{"name":"apply_patch"}}]}}
          }
        }
        """;

        var signal = ConversationRealtimeEventDecoder.Decode(json, "c1");

        Assert.Equal(ConversationRealtimeKind.Started, signal?.Kind);
        Assert.Contains("read_file", signal?.ProcessUpdate?.Title);
        Assert.Contains("apply_patch", signal?.ProcessUpdate?.Title);
    }

    [Fact]
    public void OtherConversationIsIgnored()
    {
        const string json = """
        {"type":"event","event":"turn.started","event_id":"e3","event_sequence":1,"conversation_id":"other","payload":{"kind":"chat_stream","raw":{"type":"start"}},"ts":"2026-08-30T10:00:00Z"}
        """;

        Assert.Null(ConversationRealtimeEventDecoder.Decode(json, "expected"));
    }
}
