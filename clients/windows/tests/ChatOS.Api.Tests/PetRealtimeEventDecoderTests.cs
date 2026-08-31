using ChatOS.Api.Realtime;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Tests;

public sealed class PetRealtimeEventDecoderTests
{
    [Fact]
    public void InboxUpdateMapsPersistedActivityDirectly()
    {
        const string json = """
        {
          "type":"event",
          "event":"pet_activity_inbox.updated",
          "event_id":"event-inbox-1",
          "event_sequence":99,
          "payload":{
            "kind":"pet_activity_inbox_updated",
            "activity":{
              "id":"pet_1",
              "activity_key":"task-runner:task-1",
              "activity_version":"run-2",
              "source":"task_runner",
              "kind":"succeeded",
              "title":"任务已完成",
              "route":{"task_id":"task-1","run_id":"run-2"},
              "inbox_status":"unread",
              "occurred_at":"2026-08-28T08:00:00Z",
              "updated_at":"2026-08-28T08:00:00Z"
            }
          }
        }
        """;

        var activityEvent = Assert.IsType<PetActivityEvent.Upsert>(PetRealtimeEventDecoder.Decode(json));

        Assert.Equal("pet_1", activityEvent.Activity.InboxId);
        Assert.Equal("run-2", activityEvent.Activity.ActivityVersion);
        Assert.Equal(PetActivityKind.Succeeded, activityEvent.Activity.Kind);
        Assert.Equal(99, activityEvent.Activity.EventSequence);
    }

    [Fact]
    public void ClosedInboxUpdateRemovesActivity()
    {
        const string json = """
        {
          "type":"event",
          "event_id":"event-inbox-2",
          "event_sequence":100,
          "payload":{
            "kind":"pet_activity_inbox_updated",
            "activity":{
              "id":"pet_2",
              "activity_key":"task-runner:task-2",
              "activity_version":"run-3",
              "source":"task_runner",
              "kind":"blocked",
              "title":"任务被阻塞",
              "route":{"task_id":"task-2","run_id":"run-3"},
              "inbox_status":"handled",
              "occurred_at":"2026-08-28T08:00:00Z",
              "updated_at":"2026-08-28T08:01:00Z"
            }
          }
        }
        """;

        var activityEvent = Assert.IsType<PetActivityEvent.Remove>(PetRealtimeEventDecoder.Decode(json));
        Assert.Equal("task-runner:task-2", activityEvent.Id);
    }

    [Theory]
    [InlineData("ask_user_prompt")]
    [InlineData("task_board")]
    [InlineData("chat_stream")]
    public void LegacyEventsCannotBypassPetInbox(string kind)
    {
        var json = """
        {"type":"event","event_id":"legacy","event_sequence":1,"payload":{"kind":"__KIND__"}}
        """.Replace("__KIND__", kind, StringComparison.Ordinal);

        Assert.Null(PetRealtimeEventDecoder.Decode(json));
    }
}
