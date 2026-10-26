import { describe, expect, it } from "vitest";
import type { RunInspectorGraph } from "./runInspector";
import { graphHasActiveTasks } from "./runInspector";

function graph(displayStatus: RunInspectorGraph["nodes"][number]["display_status"]): RunInspectorGraph {
  return {
    root_task_ids: ["task-1"],
    nodes: [{
      task: {
        graph_id: "graph-1",
        owner_user_id: "owner-1",
        source_entity_type: "conversation_turn",
        source_entity_id: "turn-1",
        task_id: "task-1",
        title: "Task",
        model_config_ref: "model-1",
        model_config_revision: "revision-1",
        input: {},
        status: "running",
        active_run_id: "run-1",
        version: 1,
        created_at_unix_ms: 1,
        updated_at_unix_ms: 2,
      },
      display_status: displayStatus,
      depth: 0,
      is_root: true,
      is_current_message: true,
    }],
    edges: [],
    source_conversation_id: "conversation-1",
    source_turn_id: "turn-1",
    source_user_message_id: "message-1",
  };
}

describe("run inspector task activity", () => {
  it("keeps polling while a follow-up is reviewed or awaiting confirmation", () => {
    expect(graphHasActiveTasks(graph("reviewing_followup"))).toBe(true);
    expect(graphHasActiveTasks(graph("waiting_confirmation"))).toBe(true);
    expect(graphHasActiveTasks(graph("succeeded"))).toBe(false);
  });
});
