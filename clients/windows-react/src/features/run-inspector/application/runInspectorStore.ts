import { create } from "zustand";
import type {
  RunInspectorDetail,
  RunInspectorGraph,
  RunInspectorSection,
  TaskInspectorDetail,
} from "../domain/runInspector";
import type { RunInspectorLookup, RunInspectorPort } from "../ports/RunInspectorPort";

type ProbePhase =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "unavailable" }
  | { kind: "available"; graph: RunInspectorGraph };

export type InspectorAction = "retry" | "cancel" | "restart";

export type OpenInspectorPhase = {
  kind: "open";
  lookup: RunInspectorLookup;
  graph: RunInspectorGraph;
  selectedTaskId: string;
  section: RunInspectorSection;
  taskDetail: TaskInspectorDetail | null;
  runDetail: RunInspectorDetail | null;
  isLoadingTask: boolean;
  isLoadingSection: boolean;
  isLoadingOlder: boolean;
  action: InspectorAction | null;
  retryInstruction: string;
  error: string | null;
};

type InspectorPhase = { kind: "closed" } | OpenInspectorPhase;

interface RunInspectorState {
  ownerUserId: string | null;
  lifecycleId: number;
  requestSequence: number;
  snapshotRequestId: number;
  paginationRequestId: number;
  mutationRequestId: number;
  probes: Record<string, ProbePhase>;
  phase: InspectorPhase;
  probe(port: RunInspectorPort, lookup: RunInspectorLookup): Promise<void>;
  open(port: RunInspectorPort, lookup: RunInspectorLookup): Promise<void>;
  selectTask(port: RunInspectorPort, taskId: string): Promise<void>;
  setSection(port: RunInspectorPort, section: RunInspectorSection): Promise<void>;
  setRetryInstruction(value: string): void;
  loadOlder(port: RunInspectorPort): Promise<void>;
  refresh(port: RunInspectorPort): Promise<void>;
  retry(port: RunInspectorPort): Promise<void>;
  cancel(port: RunInspectorPort): Promise<void>;
  restart(port: RunInspectorPort): Promise<void>;
  close(): void;
  reset(ownerUserId?: string): void;
}

export const useRunInspectorStore = create<RunInspectorState>((set, get) => ({
  ownerUserId: null,
  lifecycleId: 0,
  requestSequence: 0,
  snapshotRequestId: 0,
  paginationRequestId: 0,
  mutationRequestId: 0,
  probes: {},
  phase: { kind: "closed" },

  async probe(port, lookup) {
    if (get().ownerUserId !== lookup.ownerUserId) get().reset(lookup.ownerUserId);
    const current = get();
    const existing = current.probes[lookup.userMessageId];
    if (existing && existing.kind !== "idle") return;
    const lifecycleId = current.lifecycleId;
    set((state) => ({
      probes: { ...state.probes, [lookup.userMessageId]: { kind: "loading" } },
    }));
    try {
      const graph = await port.loadGraph(lookup);
      if (!acceptsLifecycle(get, lifecycleId, lookup.ownerUserId)) return;
      set((state) => ({
        probes: {
          ...state.probes,
          [lookup.userMessageId]: graph && graph.nodes.length > 0
            ? { kind: "available", graph }
            : { kind: "unavailable" },
        },
      }));
    } catch {
      if (!acceptsLifecycle(get, lifecycleId, lookup.ownerUserId)) return;
      set((state) => ({
        probes: { ...state.probes, [lookup.userMessageId]: { kind: "unavailable" } },
      }));
    }
  },

  async open(port, lookup) {
    const probe = get().probes[lookup.userMessageId];
    if (!probe || probe.kind !== "available") return;
    const selected = preferredTask(probe.graph);
    if (!selected) return;
    set({
      phase: {
        kind: "open",
        lookup,
        graph: probe.graph,
        selectedTaskId: selected.task.task_id,
        section: "detail",
        taskDetail: null,
        runDetail: null,
        isLoadingTask: false,
        isLoadingSection: false,
        isLoadingOlder: false,
        action: null,
        retryInstruction: "",
        error: null,
      },
    });
    await get().selectTask(port, selected.task.task_id);
  },

  async selectTask(port, taskId) {
    const current = get();
    if (current.phase.kind !== "open") return;
    const requestId = nextRequest(current);
    const phase = current.phase;
    set({
      requestSequence: requestId,
      snapshotRequestId: requestId,
      paginationRequestId: 0,
      phase: {
        ...phase,
        selectedTaskId: taskId,
        taskDetail: null,
        runDetail: null,
        isLoadingTask: true,
        isLoadingSection: false,
        isLoadingOlder: false,
        retryInstruction: phase.selectedTaskId === taskId ? phase.retryInstruction : "",
        error: null,
      },
    });
    try {
      const detail = await port.loadTask(phase.lookup, taskId);
      if (!acceptsSnapshot(get, requestId, taskId)) return;
      set((state) => ({
        phase: state.phase.kind === "open"
          ? { ...state.phase, taskDetail: detail, isLoadingTask: false, error: null }
          : state.phase,
      }));
      if (getOpenPhase(get)?.section === "run" && detail.task.lastRunId) {
        await loadRunSnapshot(set, get, port, requestId, taskId, detail.task.lastRunId);
      } else if (acceptsSnapshot(get, requestId, taskId)) {
        set({ snapshotRequestId: 0 });
      }
    } catch (error) {
      if (!acceptsSnapshot(get, requestId, taskId)) return;
      set((state) => ({
        snapshotRequestId: 0,
        phase: state.phase.kind === "open"
          ? { ...state.phase, isLoadingTask: false, isLoadingSection: false, error: errorMessage(error) }
          : state.phase,
      }));
    }
  },

  async setSection(port, section) {
    const current = get();
    if (current.phase.kind !== "open" || current.phase.section === section) return;
    if (!current.phase.taskDetail) {
      const taskId = current.phase.selectedTaskId;
      set({ phase: { ...current.phase, section } });
      await get().selectTask(port, taskId);
      return;
    }
    const requestId = nextRequest(current);
    const phase = current.phase;
    const runId = phase.taskDetail?.task.lastRunId ?? null;
    set({
      requestSequence: requestId,
      snapshotRequestId: section === "run" && runId ? requestId : 0,
      paginationRequestId: 0,
      phase: {
        ...phase,
        section,
        runDetail: section === "run" ? phase.runDetail : null,
        isLoadingSection: section === "run" && runId != null,
        isLoadingOlder: false,
        error: null,
      },
    });
    if (section === "run" && runId) {
      await loadRunSnapshot(set, get, port, requestId, phase.selectedTaskId, runId);
    }
  },

  setRetryInstruction(value) {
    set((state) => ({
      phase: state.phase.kind === "open"
        ? { ...state.phase, retryInstruction: value.slice(0, 8_000) }
        : state.phase,
    }));
  },

  async loadOlder(port) {
    const current = get();
    if (current.phase.kind !== "open" || !current.phase.runDetail
      || current.phase.isLoadingOlder || !current.phase.runDetail.eventsHasMore) return;
    const requestId = nextRequest(current);
    const taskId = current.phase.selectedTaskId;
    const detail = current.phase.runDetail;
    set({
      requestSequence: requestId,
      paginationRequestId: requestId,
      phase: { ...current.phase, isLoadingOlder: true, error: null },
    });
    try {
      const page = await port.loadRun(
        current.phase.lookup.ownerUserId,
        detail.run.runId,
        detail.nextOffset,
      );
      if (!acceptsPagination(get, requestId, taskId, detail.run.runId)) return;
      const eventIds = new Set(detail.events.map((event) => event.eventId));
      const merged = {
        ...page,
        events: [...detail.events, ...page.events.filter((event) => !eventIds.has(event.eventId))],
      };
      set((state) => ({
        paginationRequestId: 0,
        phase: state.phase.kind === "open"
          ? { ...state.phase, runDetail: merged, isLoadingOlder: false }
          : state.phase,
      }));
    } catch (error) {
      if (!acceptsPagination(get, requestId, taskId, detail.run.runId)) return;
      set((state) => ({
        paginationRequestId: 0,
        phase: state.phase.kind === "open"
          ? { ...state.phase, isLoadingOlder: false, error: errorMessage(error) }
          : state.phase,
      }));
    }
  },

  async refresh(port) {
    const current = get();
    if (current.phase.kind !== "open" || current.phase.action
      || current.phase.isLoadingTask || current.phase.isLoadingSection
      || current.phase.isLoadingOlder) return;
    const phase = current.phase;
    const requestId = nextRequest(current);
    set({
      requestSequence: requestId,
      snapshotRequestId: requestId,
      paginationRequestId: 0,
    });
    try {
      const fetched = await port.loadGraph(phase.lookup);
      if (!acceptsSnapshot(get, requestId, phase.selectedTaskId)) return;
      const graph = preserveStableGraph(phase.graph, fetched);
      const selectedTaskId = graph.nodes.some(({ task }) => task.task_id === phase.selectedTaskId)
        ? phase.selectedTaskId
        : preferredTask(graph)?.task.task_id;
      if (!selectedTaskId) return;
      set((state) => ({
        probes: {
          ...state.probes,
          [phase.lookup.userMessageId]: { kind: "available", graph },
        },
        phase: state.phase.kind === "open"
          ? { ...state.phase, graph, selectedTaskId }
          : state.phase,
      }));
      await get().selectTask(port, selectedTaskId);
    } catch (error) {
      if (!acceptsSnapshot(get, requestId, phase.selectedTaskId)) return;
      set((state) => ({
        snapshotRequestId: 0,
        phase: state.phase.kind === "open"
          ? { ...state.phase, error: errorMessage(error) }
          : state.phase,
      }));
    }
  },

  async retry(port) {
    await mutateSelected(set, get, port, "retry");
  },

  async cancel(port) {
    await mutateSelected(set, get, port, "cancel");
  },

  async restart(port) {
    await mutateSelected(set, get, port, "restart");
  },

  close() {
    set((state) => ({
      requestSequence: state.requestSequence + 1,
      snapshotRequestId: 0,
      paginationRequestId: 0,
      mutationRequestId: 0,
      phase: { kind: "closed" },
    }));
  },

  reset(ownerUserId = "") {
    set((state) => ({
      ownerUserId: ownerUserId.trim() || null,
      lifecycleId: state.lifecycleId + 1,
      requestSequence: state.requestSequence + 1,
      snapshotRequestId: 0,
      paginationRequestId: 0,
      mutationRequestId: 0,
      probes: {},
      phase: { kind: "closed" },
    }));
  },
}));

type SetState = Parameters<typeof useRunInspectorStore.setState>[0] extends infer _Value
  ? (value: Partial<RunInspectorState> | ((state: RunInspectorState) => Partial<RunInspectorState>)) => void
  : never;
type GetState = () => RunInspectorState;

async function loadRunSnapshot(
  set: SetState,
  get: GetState,
  port: RunInspectorPort,
  requestId: number,
  taskId: string,
  runId: string,
) {
  set((state) => ({
    phase: state.phase.kind === "open"
      ? { ...state.phase, isLoadingSection: true }
      : state.phase,
  }));
  try {
    const detail = await port.loadRun(getOpenPhase(get)!.lookup.ownerUserId, runId, 0);
    if (!acceptsSnapshot(get, requestId, taskId)
      || getOpenPhase(get)?.section !== "run") return;
    set((state) => ({
      snapshotRequestId: 0,
      phase: state.phase.kind === "open"
        ? { ...state.phase, runDetail: detail, isLoadingSection: false, error: null }
        : state.phase,
    }));
  } catch (error) {
    if (!acceptsSnapshot(get, requestId, taskId)
      || getOpenPhase(get)?.section !== "run") return;
    set((state) => ({
      snapshotRequestId: 0,
      phase: state.phase.kind === "open"
        ? { ...state.phase, isLoadingSection: false, error: errorMessage(error) }
        : state.phase,
    }));
  }
}

async function mutateSelected(
  set: SetState,
  get: GetState,
  port: RunInspectorPort,
  action: InspectorAction,
) {
  const current = get();
  if (current.phase.kind !== "open" || current.phase.action || !current.phase.taskDetail) return;
  const phase = current.phase;
  const taskDetail = current.phase.taskDetail;
  const requestId = nextRequest(current);
  const request = {
    ownerUserId: phase.lookup.ownerUserId,
    taskId: phase.selectedTaskId,
    expectedVersion: taskDetail.task.version,
    instruction: action === "retry" ? phase.retryInstruction : undefined,
    reason: action === "cancel"
      ? "user requested cancellation from task inspector"
      : action === "restart" ? "user requested recovery from task inspector" : undefined,
  };
  set({
    requestSequence: requestId,
    mutationRequestId: requestId,
    snapshotRequestId: 0,
    paginationRequestId: 0,
    phase: { ...phase, action, error: null },
  });
  try {
    if (action === "retry") await port.retryTask(request);
    else if (action === "cancel") await port.cancelTask(request);
    else await port.restartTask(request);
    if (!acceptsMutation(get, requestId, phase.selectedTaskId)) return;
    set((state) => ({
      mutationRequestId: 0,
      phase: state.phase.kind === "open"
        ? {
            ...state.phase,
            action: null,
            retryInstruction: action === "retry" ? "" : state.phase.retryInstruction,
            section: action === "retry" || action === "restart" ? "run" : state.phase.section,
          }
        : state.phase,
    }));
    await get().refresh(port);
  } catch (error) {
    if (!acceptsMutation(get, requestId, phase.selectedTaskId)) return;
    set((state) => ({
      mutationRequestId: 0,
      phase: state.phase.kind === "open"
        ? { ...state.phase, action: null, error: errorMessage(error) }
        : state.phase,
    }));
  }
}

function preferredTask(graph: RunInspectorGraph) {
  return graph.nodes.find((node) => node.is_current_message)
    ?? graph.nodes.find((node) => node.is_root)
    ?? graph.nodes[0];
}

function preserveStableGraph(current: RunInspectorGraph, fetched: RunInspectorGraph | null) {
  if (!fetched || (fetched.nodes.length === 0 && current.nodes.length > 0)) return current;
  return fetched;
}

function nextRequest(state: RunInspectorState) {
  return state.requestSequence + 1;
}

function getOpenPhase(get: GetState) {
  const phase = get().phase;
  return phase.kind === "open" ? phase : null;
}

function acceptsLifecycle(get: GetState, lifecycleId: number, ownerUserId: string) {
  const state = get();
  return state.lifecycleId === lifecycleId && state.ownerUserId === ownerUserId;
}

function acceptsSnapshot(get: GetState, requestId: number, taskId: string) {
  const state = get();
  return state.snapshotRequestId === requestId
    && state.phase.kind === "open"
    && state.phase.selectedTaskId === taskId;
}

function acceptsPagination(get: GetState, requestId: number, taskId: string, runId: string) {
  const state = get();
  return state.paginationRequestId === requestId
    && state.phase.kind === "open"
    && state.phase.selectedTaskId === taskId
    && state.phase.section === "run"
    && state.phase.runDetail?.run.runId === runId;
}

function acceptsMutation(get: GetState, requestId: number, taskId: string) {
  const state = get();
  return state.mutationRequestId === requestId
    && state.phase.kind === "open"
    && state.phase.selectedTaskId === taskId;
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "任务检查器加载失败。";
}
