using ChatOS.Api.Conversation;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Tests;

public sealed class ConversationHistoryServiceTests
{
    [Fact]
    public async Task CompactHistoryAssociatesFinalReplyCallbacksAndAttachments()
    {
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            Assert.Equal("/api/chatos/conversations/c%2F1/compact-history", request.RequestUri?.AbsolutePath);
            Assert.Equal("?limit=10&before=cursor%2F1", request.RequestUri?.Query);
            return StubHttpMessageHandler.Json("""
                {
                  "items":[
                    {
                      "id":"user-1",
                      "conversation_id":"c/1",
                      "turn_id":"turn-1",
                      "sequence_no":4,
                      "revision":3,
                      "role":"user",
                      "content":"处理这个任务",
                      "created_at":"2026-08-30T10:00:00Z",
                      "metadata":{
                        "historyProcess":{"finalAssistantMessageId":"assistant-1","processMessageCount":2},
                        "project_requirement_execution":{"project_id":"project-1","requirement_id":"req-1","execution_group_id":"group-1"},
                        "attachments":[{"id":"file-1","name":"spec.md","mimeType":"text/markdown","size":12,"type":"file","viewUrl":"https://files/spec.md"}]
                      }
                    },
                    {
                      "id":"assistant-1",
                      "turn_id":"turn-1",
                      "revision":5,
                      "role":"assistant",
                      "content":"已经开始处理",
                      "status":"completed",
                      "created_at":"2026-08-30T10:00:05Z",
                      "metadata":{"historyFinalForUserMessageId":"user-1","historyFinalForTurnId":"turn-1"}
                    },
                    {
                      "id":"callback-1",
                      "role":"assistant",
                      "content":"任务执行完成",
                      "message_mode":"task_runner_callback",
                      "created_at":"2026-08-30T10:01:00Z",
                      "metadata":{"task_runner_async":{"task_id":"task-1","run_id":"run-1","event":"task.completed","source_turn_id":"turn-1","source_user_message_id":"user-1"}}
                    },
                    {
                      "id":"callback-cancelled",
                      "role":"assistant",
                      "content":"不应出现",
                      "message_mode":"task_runner_callback",
                      "metadata":{"task_runner_async":{"task_id":"task-2","event":"task.cancelled","source_turn_id":"turn-1"}}
                    }
                  ],
                  "has_more":true,
                  "next_before":"cursor-older",
                  "snapshot_revision":9
                }
                """);
        });
        var service = new ConversationHistoryService(client);

        var page = await service.FetchHistoryAsync(new ConversationHistoryQuery(
            "c/1",
            10,
            "cursor/1",
            6));

        var turn = Assert.Single(page.Turns);
        Assert.Equal("turn-1", turn.Id);
        Assert.Equal(4, turn.Sequence);
        Assert.True(turn.Revision > 5);
        Assert.Equal(TurnStatus.Completed, turn.Status);
        Assert.Equal("已经开始处理", turn.FinalAssistantMessage?.Text);
        Assert.Equal(2, turn.AssistantReplies.Count);
        Assert.Equal("completed", turn.AssistantReplies[1].TaskCallback?.Status);
        Assert.DoesNotContain(turn.AssistantReplies, reply => reply.Message.Id == "callback-cancelled");
        Assert.Equal("spec.md", Assert.Single(turn.UserMessage.Attachments).Name);
        Assert.Equal("group-1", turn.ProjectExecutionContext?.ExecutionGroupId);
        Assert.Single(turn.ProcessEvents);
        Assert.True(page.HasOlder);
        Assert.Equal("cursor-older", page.OlderCursor);
        Assert.Equal(9, page.SnapshotRevision);
        Assert.Equal(6, page.RequestGeneration);
    }

    [Fact]
    public async Task UserWithoutAssistantRemainsStreaming()
    {
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, _ => StubHttpMessageHandler.Json("""
            {"items":[{"id":"u1","role":"user","content":"hello","metadata":{}}],"has_more":false}
            """));
        var service = new ConversationHistoryService(client);

        var turn = Assert.Single((await service.FetchHistoryAsync(
            new ConversationHistoryQuery("c1", 10, null, 1))).Turns);

        Assert.Equal(TurnStatus.Streaming, turn.Status);
        Assert.Null(turn.FinalAssistantMessage);
    }
}
