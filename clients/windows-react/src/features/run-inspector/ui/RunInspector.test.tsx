import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { useRunInspectorStore } from "../application/runInspectorStore";
import type { RunInspectorGraph } from "../domain/runInspector";
import type { RunInspectorPort } from "../ports/RunInspectorPort";
import { RunInspectorLauncher } from "./RunInspector";

const lookup = {
  ownerUserId: "owner-1",
  conversationId: "conversation-1",
  turnId: "turn-1",
  userMessageId: "message-1",
};

const graph: RunInspectorGraph = {
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
    display_status: "waiting_confirmation",
    depth: 0,
    is_root: true,
    is_current_message: true,
  }],
  edges: [],
  source_conversation_id: "conversation-1",
  source_turn_id: "turn-1",
  source_user_message_id: "message-1",
};

describe("Run Inspector launcher", () => {
  beforeEach(() => useRunInspectorStore.getState().reset("owner-1"));
  afterEach(cleanup);

  it("shows the task-level confirmation gate in the conversation timeline", async () => {
    render(<RunInspectorLauncher lookup={lookup} port={port()} />);
    expect(await screen.findByText("任务 · 等待确认")).toBeVisible();
    expect(screen.getByRole("button", { name: /任务 · 等待确认.*运行详情/ })).toHaveClass(
      "run-inspector-launcher--attention",
    );
  });
});

function port(): RunInspectorPort {
  return {
    loadGraph: async () => graph,
    loadTask: async () => { throw new Error("not used"); },
    loadRun: async () => { throw new Error("not used"); },
    retryTask: async () => { throw new Error("not used"); },
    cancelTask: async () => { throw new Error("not used"); },
    restartTask: async () => { throw new Error("not used"); },
  };
}
