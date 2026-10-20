import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  RunInspectorDetail,
  RunInspectorGraph,
  TaskInspectorDetail,
} from "../domain/runInspector";
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

const taskDetail: TaskInspectorDetail = {
  task: {
    taskId: "task-1",
    title: "Inspect",
    status: "running",
    version: 1,
    description: "description",
    objective: "objective",
    resultSummary: null,
    processEntries: [],
    lastRunId: "run-1",
    modelConfigRef: "model-1",
    thinkingLevel: "medium",
    createdAtUnixMs: 1,
    updatedAtUnixMs: 2,
  },
  run: null,
};

const latest: RunInspectorDetail = {
  run: {
    runId: "run-1",
    taskId: "task-1",
    status: "model_running",
    iteration: 2,
    maxIterations: 8,
    resultSummary: null,
    reportContent: null,
    errorMessage: null,
    createdAtUnixMs: 100,
    updatedAtUnixMs: 103,
    finishedAtUnixMs: null,
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

  it("loads task details before the selected section", async () => {
    const calls: string[] = [];
    const port = stubPort({
      loadTask: async () => {
        calls.push("task");
        return taskDetail;
      },
      loadRun: async () => {
        calls.push("run");
        return latest;
      },
    });
    await open(port);
    expect(calls).toEqual(["task"]);
    await useRunInspectorStore.getState().setSection(port, "run");
    expect(calls).toEqual(["task", "run"]);
    expect(openPhase().runDetail?.events[0]?.eventId).toBe("event-103");
  });

  it("merges older events by durable event identity", async () => {
    const older = {
      ...latest,
      events: [
        latest.events[1]!,
        { cursor: 101, eventId: "event-101", eventType: "model.updated", summary: null, status: "model_running", createdAtUnixMs: 101 },
      ],
      eventsHasMore: false,
      nextOffset: 4,
    };
    const port = stubPort({
      loadRun: async (_owner, _run, offset) => offset === 0 ? latest : older,
    });
    await open(port);
    await useRunInspectorStore.getState().setSection(port, "run");
    await useRunInspectorStore.getState().loadOlder(port);
    expect(openPhase().runDetail?.events.map((event) => event.eventId)).toEqual([
      "event-103", "event-102", "event-101",
    ]);
  });

  it("preserves a stable graph across a transient empty refresh", async () => {
    let graphCalls = 0;
    const port = stubPort({
      loadGraph: async () => {
        graphCalls += 1;
        return graphCalls === 1 ? graph : { ...graph, nodes: [] };
      },
    });
    await open(port);
    await useRunInspectorStore.getState().refresh(port);
    expect(openPhase().graph.nodes).toHaveLength(1);
    expect(openPhase().selectedTaskId).toBe("task-1");
  });

  it("rejects a late task response after the inspector closes", async () => {
    let resolveTask!: (value: TaskInspectorDetail) => void;
    const deferred = new Promise<TaskInspectorDetail>((resolve) => { resolveTask = resolve; });
    const port = stubPort({ loadTask: async () => deferred });
    await useRunInspectorStore.getState().probe(port, lookup);
    const opening = useRunInspectorStore.getState().open(port, lookup);
    useRunInspectorStore.getState().close();
    resolveTask(taskDetail);
    await opening;
    expect(useRunInspectorStore.getState().phase.kind).toBe("closed");
  });

  it("rejects late run results after switching sections", async () => {
    let resolveRun!: (value: RunInspectorDetail) => void;
    const deferred = new Promise<RunInspectorDetail>((resolve) => { resolveRun = resolve; });
    const port = stubPort({ loadRun: async () => deferred });
    await open(port);
    const loading = useRunInspectorStore.getState().setSection(port, "run");
    await useRunInspectorStore.getState().setSection(port, "process");
    resolveRun(latest);
    await loading;
    expect(openPhase().section).toBe("process");
    expect(openPhase().runDetail).toBeNull();
  });

  it("uses the current task version and refreshes after retry", async () => {
    const retryTask = vi.fn(async () => ({
      taskId: "task-1",
      status: "ready",
      activeRunId: "run-2",
      version: 2,
    }));
    const port = stubPort({ retryTask });
    await open(port);
    useRunInspectorStore.getState().setRetryInstruction("use current data");
    await useRunInspectorStore.getState().retry(port);
    expect(retryTask).toHaveBeenCalledWith(expect.objectContaining({
      taskId: "task-1",
      expectedVersion: 1,
      instruction: "use current data",
    }));
    expect(openPhase().retryInstruction).toBe("");
    expect(openPhase().section).toBe("run");
  });
});

function stubPort(overrides: Partial<RunInspectorPort> = {}): RunInspectorPort {
  return {
    loadGraph: async () => graph,
    loadTask: async () => taskDetail,
    loadRun: async () => latest,
    retryTask: async () => ({ taskId: "task-1", status: "ready", activeRunId: "run-2", version: 2 }),
    cancelTask: async () => ({ taskId: "task-1", status: "cancelled", activeRunId: null, version: 2 }),
    restartTask: async () => ({ taskId: "task-1", status: "ready", activeRunId: "run-2", version: 2 }),
    ...overrides,
  };
}

async function open(port: RunInspectorPort) {
  await useRunInspectorStore.getState().probe(port, lookup);
  await useRunInspectorStore.getState().open(port, lookup);
}

function openPhase() {
  const phase = useRunInspectorStore.getState().phase;
  if (phase.kind !== "open") throw new Error("inspector is closed");
  return phase;
}
