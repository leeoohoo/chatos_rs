import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { OpenInspectorPhase } from "../application/runInspectorStore";
import { InspectorHeaderActions, InspectorRecoveryActions } from "./RunInspectorActions";

const phase: OpenInspectorPhase = {
  kind: "open",
  lookup: {
    ownerUserId: "user-1",
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
      title: "Inspect",
      status: "failed",
      version: 3,
      description: null,
      objective: null,
      resultSummary: null,
      processEntries: [],
      lastRunId: "run-1",
      modelConfigRef: "model-1",
      thinkingLevel: null,
      createdAtUnixMs: 1,
      updatedAtUnixMs: 2,
    },
    run: null,
  },
  runDetail: null,
  isLoadingTask: false,
  isLoadingSection: false,
  isLoadingOlder: false,
  action: null,
  retryInstruction: "",
  error: null,
};

describe("Run Inspector actions", () => {
  afterEach(cleanup);

  it("collects an optional retry instruction before retrying a failed task", () => {
    const instruction = vi.fn();
    const retry = vi.fn();
    render(
      <InspectorRecoveryActions
        phase={phase}
        onInstruction={instruction}
        onRestart={vi.fn()}
        onRetry={retry}
      />,
    );
    fireEvent.change(screen.getByLabelText("重试说明"), { target: { value: "换一种方式" } });
    fireEvent.click(screen.getByRole("button", { name: "重新运行" }));
    expect(instruction).toHaveBeenCalledWith("换一种方式");
    expect(retry).toHaveBeenCalledOnce();
  });

  it("requires explicit confirmation before cancelling an active task", () => {
    const cancel = vi.fn();
    render(
      <InspectorHeaderActions
        phase={{
          ...phase,
          taskDetail: phase.taskDetail
            ? { ...phase.taskDetail, task: { ...phase.taskDetail.task, status: "running" } }
            : null,
        }}
        onCancel={cancel}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "◼ 停止" }));
    expect(cancel).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "停止" }));
    expect(cancel).toHaveBeenCalledOnce();
  });
});
