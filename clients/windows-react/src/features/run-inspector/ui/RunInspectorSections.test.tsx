import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { OpenInspectorPhase } from "../application/runInspectorStore";
import { TaskDetailSection } from "./RunInspectorSections";

const phase: OpenInspectorPhase = {
  kind: "open",
  lookup: {
    ownerUserId: "owner-1",
    conversationId: "conversation-1",
    turnId: "turn-1",
    userMessageId: "message-1",
  },
  graph: {
    root_task_ids: [],
    nodes: [],
    edges: [],
    source_conversation_id: "conversation-1",
    source_turn_id: "turn-1",
    source_user_message_id: "message-1",
  },
  selectedTaskId: "task-1",
  section: "detail",
  taskDetail: {
    task: {
      taskId: "task-1",
      title: "Task",
      status: "reviewing_followup",
      version: 2,
      description: null,
      objective: "Review the follow-up",
      resultSummary: "stale task summary",
      processEntries: [],
      lastRunId: "run-1",
      modelConfigRef: "model-1",
      thinkingLevel: null,
      createdAtUnixMs: 1,
      updatedAtUnixMs: 2,
    },
    run: {
      runId: "run-1",
      taskId: "task-1",
      status: "model_running",
      iteration: 2,
      maxIterations: 8,
      resultSummary: "stale run summary",
      reportContent: "stale model output",
      errorMessage: null,
      createdAtUnixMs: 1,
      updatedAtUnixMs: 2,
      finishedAtUnixMs: null,
    },
  },
  runDetail: null,
  isLoadingTask: false,
  isLoadingSection: false,
  isLoadingOlder: false,
  action: null,
  retryInstruction: "",
  error: null,
};

describe("Run Inspector task detail", () => {
  afterEach(cleanup);

  it("prioritizes a follow-up gate over stale run output", () => {
    render(<TaskDetailSection phase={phase} />);
    expect(screen.getByRole("heading", { name: "审查追加要求" })).toBeInTheDocument();
    expect(screen.getByText("后续执行已暂停，正在审查追加要求。")).toBeInTheDocument();
    expect(screen.queryByText("stale model output")).not.toBeInTheDocument();
    expect(screen.queryByText("stale task summary")).not.toBeInTheDocument();
  });
});
