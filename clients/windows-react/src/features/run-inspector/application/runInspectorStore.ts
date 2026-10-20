import { create } from "zustand";
import type { RunInspectorDetail, RunInspectorGraph } from "../domain/runInspector";
import type { RunInspectorLookup, RunInspectorPort } from "../ports/RunInspectorPort";

type ProbePhase =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "unavailable" }
  | { kind: "available"; graph: RunInspectorGraph };

type InspectorPhase =
  | { kind: "closed" }
  | {
      kind: "open";
      lookup: RunInspectorLookup;
      graph: RunInspectorGraph;
      selectedTaskId: string;
      detail: RunInspectorDetail | null;
      isLoading: boolean;
      isLoadingOlder: boolean;
      error: string | null;
    };

interface RunInspectorState {
  ownerUserId: string | null;
  requestSequence: number;
  requestId: number;
  probes: Record<string, ProbePhase>;
  phase: InspectorPhase;
  probe(port: RunInspectorPort, lookup: RunInspectorLookup): Promise<void>;
  open(port: RunInspectorPort, lookup: RunInspectorLookup): Promise<void>;
  selectTask(port: RunInspectorPort, taskId: string): Promise<void>;
  loadOlder(port: RunInspectorPort): Promise<void>;
  refresh(port: RunInspectorPort): Promise<void>;
  close(): void;
  reset(ownerUserId?: string): void;
}

export const useRunInspectorStore = create<RunInspectorState>((set, get) => ({
  ownerUserId: null,
  requestSequence: 0,
  requestId: 0,
  probes: {},
  phase: { kind: "closed" },

  async probe(port, lookup) {
    if (get().ownerUserId !== lookup.ownerUserId) get().reset(lookup.ownerUserId);
    const current = get().probes[lookup.userMessageId];
    if (current && current.kind !== "idle") return;
    set((state) => ({ probes: { ...state.probes, [lookup.userMessageId]: { kind: "loading" } } }));
    try {
      const graph = await port.loadGraph(lookup);
      if (get().ownerUserId !== lookup.ownerUserId) return;
      set((state) => ({
        probes: {
          ...state.probes,
          [lookup.userMessageId]: graph && graph.nodes.length > 0
            ? { kind: "available", graph }
            : { kind: "unavailable" },
        },
      }));
    } catch {
      if (get().ownerUserId !== lookup.ownerUserId) return;
      set((state) => ({
        probes: { ...state.probes, [lookup.userMessageId]: { kind: "unavailable" } },
      }));
    }
  },

  async open(port, lookup) {
    const probe = get().probes[lookup.userMessageId];
    if (!probe || probe.kind !== "available") return;
    const selected = probe.graph.nodes.find((node) => node.is_current_message)
      ?? probe.graph.nodes.find((node) => node.is_root)
      ?? probe.graph.nodes[0];
    if (!selected) return;
    set({
      phase: {
        kind: "open",
        lookup,
        graph: probe.graph,
        selectedTaskId: selected.task.task_id,
        detail: null,
        isLoading: false,
        isLoadingOlder: false,
        error: null,
      },
    });
    await get().selectTask(port, selected.task.task_id);
  },

  async selectTask(port, taskId) {
    const current = get();
    if (current.phase.kind !== "open") return;
    const task = current.phase.graph.nodes.find((node) => node.task.task_id === taskId)?.task;
    const requestId = current.requestSequence + 1;
    set({
      requestSequence: requestId,
      requestId,
      phase: {
        ...current.phase,
        selectedTaskId: taskId,
        detail: null,
        isLoading: task?.active_run_id != null,
        isLoadingOlder: false,
        error: null,
      },
    });
    if (!task?.active_run_id) {
      set({ requestId: 0 });
      return;
    }
    try {
      const detail = await port.loadRun(current.phase.lookup.ownerUserId, task.active_run_id, 0);
      if (!accepts(get, requestId, taskId)) return;
      set((state) => ({
        requestId: 0,
        phase: state.phase.kind === "open"
          ? { ...state.phase, detail, isLoading: false, error: null }
          : state.phase,
      }));
    } catch (error) {
      if (!accepts(get, requestId, taskId)) return;
      set((state) => ({
        requestId: 0,
        phase: state.phase.kind === "open"
          ? { ...state.phase, isLoading: false, error: errorMessage(error) }
          : state.phase,
      }));
    }
  },

  async loadOlder(port) {
    const current = get();
    if (current.phase.kind !== "open" || !current.phase.detail
      || current.phase.isLoadingOlder || !current.phase.detail.eventsHasMore) return;
    const requestId = current.requestSequence + 1;
    const taskId = current.phase.selectedTaskId;
    const detail = current.phase.detail;
    set({
      requestSequence: requestId,
      requestId,
      phase: { ...current.phase, isLoadingOlder: true, error: null },
    });
    try {
      const page = await port.loadRun(
        current.phase.lookup.ownerUserId,
        detail.run.runId,
        detail.nextOffset,
      );
      if (!accepts(get, requestId, taskId)) return;
      const eventIds = new Set(detail.events.map((event) => event.eventId));
      const merged = {
        ...page,
        events: [...detail.events, ...page.events.filter((event) => !eventIds.has(event.eventId))],
      };
      set((state) => ({
        requestId: 0,
        phase: state.phase.kind === "open"
          ? { ...state.phase, detail: merged, isLoadingOlder: false }
          : state.phase,
      }));
    } catch (error) {
      if (!accepts(get, requestId, taskId)) return;
      set((state) => ({
        requestId: 0,
        phase: state.phase.kind === "open"
          ? { ...state.phase, isLoadingOlder: false, error: errorMessage(error) }
          : state.phase,
      }));
    }
  },

  async refresh(port) {
    const phase = get().phase;
    if (phase.kind !== "open" || phase.isLoading || phase.isLoadingOlder) return;
    await get().selectTask(port, phase.selectedTaskId);
  },

  close() {
    set((state) => ({
      requestSequence: state.requestSequence + 1,
      requestId: 0,
      phase: { kind: "closed" },
    }));
  },

  reset(ownerUserId = "") {
    set((state) => ({
      ownerUserId: ownerUserId.trim() || null,
      requestSequence: state.requestSequence + 1,
      requestId: 0,
      probes: {},
      phase: { kind: "closed" },
    }));
  },
}));

function accepts(get: () => RunInspectorState, requestId: number, taskId: string) {
  const state = get();
  return state.requestId === requestId
    && state.phase.kind === "open"
    && state.phase.selectedTaskId === taskId;
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "运行详情加载失败。";
}
