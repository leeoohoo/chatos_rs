import { beforeEach, describe, expect, it } from "vitest";
import type { RunInspectorGraph } from "../domain/runInspector";
import type { RunInspectorPort } from "../ports/RunInspectorPort";
import { useRunInspectorStore } from "./runInspectorStore";

const lookup = {
  ownerUserId: "user-1",
  conversationId: "conversation-1",
  turnId: "turn-1",
  userMessageId: "message-user-1",
};

const graph: RunInspectorGraph = {
  root_task_ids: ["task-1"],
  nodes: [{
    task: {
      graph_id: "graph-1",
      owner_user_id: "user-1",
      source_entity_type: "conversation_turn",
      source_entity_id: "turn-1",
      task_id: "task-1",
      title: "Inspect",
      model_config_ref: "model-1",
      model_config_revision: "revision-1",
      input: {},
      status: "running",
      active_run_id: "run-1",
      version: 1,
      created_at_unix_ms: 1,
      updated_at_unix_ms: 1,
    },
    depth: 0,
    is_root: true,
    is_current_message: true,
  }],
  edges: [],
  source_conversation_id: "conversation-1",
  source_turn_id: "turn-1",
  source_user_message_id: "message-user-1",
};

const latest = {
  run: {
    runId: "run-1",
    taskId: "task-1",
    status: "model_running",
    iteration: 2,
    maxIterations: 8,
    updatedAtUnixMs: 103,
  },
  events: [
    { cursor: 103, eventId: "event-103", eventType: "tool.completed", summary: "done", status: "completed", createdAtUnixMs: 103 },
    { cursor: 102, eventId: "event-102", eventType: "tool.started", summary: null, status: "running", createdAtUnixMs: 102 },
  ],
  eventsHasMore: true,
  nextOffset: 2,
};

describe("run inspector store", () => {
  beforeEach(() => useRunInspectorStore.getState().reset("user-1"));

  it("shows only a graph-backed message and loads the newest event page first", async () => {
    const requests: number[] = [];
    const port: RunInspectorPort = {
      loadGraph: async () => graph,
      loadRun: async (_owner, _run, offset) => {
        requests.push(offset);
        return latest;
      },
    };

    await useRunInspectorStore.getState().probe(port, lookup);
    await useRunInspectorStore.getState().open(port, lookup);

    expect(requests).toEqual([0]);
    const phase = useRunInspectorStore.getState().phase;
    expect(phase.kind).toBe("open");
    if (phase.kind === "open") {
      expect(phase.detail?.events.map((event) => event.cursor)).toEqual([103, 102]);
    }
  });

  it("merges older events by event identity", async () => {
    const older = {
      ...latest,
      events: [
        latest.events[1]!,
        { cursor: 101, eventId: "event-101", eventType: "model.updated", summary: null, status: "model_running", createdAtUnixMs: 101 },
      ],
      eventsHasMore: false,
      nextOffset: 4,
    };
    const port: RunInspectorPort = {
      loadGraph: async () => graph,
      loadRun: async (_owner, _run, offset) => offset === 0 ? latest : older,
    };

    await useRunInspectorStore.getState().probe(port, lookup);
    await useRunInspectorStore.getState().open(port, lookup);
    await useRunInspectorStore.getState().loadOlder(port);

    const phase = useRunInspectorStore.getState().phase;
    expect(phase.kind).toBe("open");
    if (phase.kind === "open") {
      expect(phase.detail?.events.map((event) => event.eventId)).toEqual([
        "event-103",
        "event-102",
        "event-101",
      ]);
    }
  });
});
