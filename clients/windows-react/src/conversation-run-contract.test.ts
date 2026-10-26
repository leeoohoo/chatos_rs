import { describe, expect, it } from "vitest";
import conversationHistory from "../../contracts/local-agent/v48/fixtures/conversation-history-success.json";
import messageTaskGraph from "../../contracts/local-agent/v48/fixtures/message-task-graph-success.json";
import runEventsLatest from "../../contracts/local-agent/v48/fixtures/run-events-latest-success.json";
import {
  conversationHistoryResponseSchema,
  messageTaskGraphResponseSchema,
  runEventsResponseSchema,
} from "./contracts/conversationRunContract";

describe("Conversation and Run shared Host v48 fixtures", () => {
  it("preserves conversation, turn, message, task, and run links", () => {
    const history = conversationHistoryResponseSchema.parse(conversationHistory).result.page;
    const graph = messageTaskGraphResponseSchema.parse(messageTaskGraph).result.graph;

    expect(graph.source_conversation_id).toBe(history.conversation.conversation_id);
    expect(graph.source_turn_id).toBe(history.turns[0]?.turn_id);
    expect(graph.source_user_message_id).toBe(history.messages[0]?.message_id);
    expect(graph.nodes[0]?.task.active_run_id).toBe(history.turns[0]?.run_id);
    expect(history.attachments[0]?.authorized_local_ref).toBe(
      "local-attachment:authority-1",
    );
  });

  it("keeps the first Run event page newest-first with an older cursor", () => {
    const result = runEventsResponseSchema.parse(runEventsLatest).result;
    const cursors = result.events.map((event) => event.cursor);

    expect(cursors).toEqual([103, 102, 101]);
    expect(result.events.every((event) => event.run_id === "run-1")).toBe(true);
    expect(result.next_cursor).toBe(cursors.at(-1));
  });
});
