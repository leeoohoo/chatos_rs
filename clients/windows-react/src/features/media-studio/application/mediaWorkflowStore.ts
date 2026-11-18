import { create } from "zustand";
import {
  mediaWorkflowSchema,
  newMediaWorkflow,
  newMediaWorkflowNode,
  type MediaWorkflow,
  type MediaWorkflowNodeRunResult,
  type MediaWorkflowNodeKind,
  type MediaWorkflowRunEvent,
} from "../domain/mediaWorkflow";
import type { MediaStudioPort } from "../ports/MediaStudioPort";
import { useMediaStudioStore } from "./mediaStudioStore";

export type MediaWorkflowNodeState =
  | { phase: "running"; progress: string | null }
  | { phase: "completed"; result: MediaWorkflowNodeRunResult }
  | { phase: "failed"; errorMessage: string };

interface MediaWorkflowState {
  ownerUserId: string | null;
  workflows: MediaWorkflow[];
  selectedWorkflowId: string | null;
  draft: MediaWorkflow | null;
  isLoading: boolean;
  isSaving: boolean;
  isRunning: boolean;
  nodeStates: Record<string, MediaWorkflowNodeState>;
  runResult: import("../domain/mediaWorkflow").MediaWorkflowRunResult | null;
  activeRunRequestId: string | null;
  activeSaveRequestId: string | null;
  errorMessage: string | null;
  activate(port: MediaStudioPort, ownerUserId: string): Promise<void>;
  create(modelConfigId?: string, kind?: MediaWorkflowNodeKind): void;
  open(workflow: MediaWorkflow): void;
  close(): void;
  updateDraft(update: (workflow: MediaWorkflow) => MediaWorkflow): void;
  addNode(kind: MediaWorkflowNodeKind, modelConfigId?: string): void;
  removeNode(nodeId: string): void;
  moveNode(nodeId: string, direction: -1 | 1): void;
  save(port: MediaStudioPort): Promise<boolean>;
  run(port: MediaStudioPort): Promise<void>;
  remove(port: MediaStudioPort, workflowId?: string): Promise<void>;
  dismissError(): void;
  reset(): void;
}

function errorMessage(error: unknown) {
  if (error instanceof Error) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "工作流操作失败。";
}

export const useMediaWorkflowStore = create<MediaWorkflowState>((set, get) => ({
  ownerUserId: null,
  workflows: [],
  selectedWorkflowId: null,
  draft: null,
  isLoading: false,
  isSaving: false,
  isRunning: false,
  nodeStates: {},
  runResult: null,
  activeRunRequestId: null,
  activeSaveRequestId: null,
  errorMessage: null,

  async activate(port, ownerInput) {
    const ownerUserId = ownerInput.trim();
    if (!ownerUserId) {
      get().reset();
      return;
    }
    if (get().ownerUserId === ownerUserId && !get().isLoading) return;
    set({ ownerUserId, isLoading: true, errorMessage: null, selectedWorkflowId: null, draft: null, nodeStates: {}, runResult: null });
    try {
      const workflows = await port.listWorkflows(ownerUserId);
      if (get().ownerUserId !== ownerUserId) return;
      set({ workflows, isLoading: false });
    } catch (error) {
      if (get().ownerUserId !== ownerUserId) return;
      set({ workflows: [], isLoading: false, errorMessage: errorMessage(error) });
    }
  },

  create(modelConfigId = "", kind = "image") {
    const existing = get().workflows.length;
    const workflow = newMediaWorkflow(modelConfigId, kind);
    workflow.name = `工作流 ${existing + 1}`;
    set({ selectedWorkflowId: workflow.id, draft: workflow, errorMessage: null, runResult: null, nodeStates: {} });
  },

  open(workflow) {
    const parsed = mediaWorkflowSchema.parse(workflow);
    set({ selectedWorkflowId: parsed.id, draft: structuredClone(parsed), errorMessage: null, runResult: null, nodeStates: {} });
  },

  close() {
    if (get().isSaving || get().isRunning) return;
    set({ selectedWorkflowId: null, draft: null, errorMessage: null, runResult: null, nodeStates: {} });
  },

  updateDraft(update) {
    const current = get().draft;
    if (!current || get().isSaving || get().isRunning) return;
    const next = update(structuredClone(current));
    set({ draft: { ...next, updatedAtUnixMs: Date.now() } });
  },

  addNode(kind, modelConfigId = "") {
    get().updateDraft((workflow) => {
      if (workflow.nodes.length >= 8 || workflow.nodes.some((node) => node.kind === "video")) return workflow;
      const node = newMediaWorkflowNode(kind, modelConfigId);
      if (kind === "video") workflow.nodes.push(node);
      else workflow.nodes.push(node);
      return workflow;
    });
  },

  removeNode(nodeId) {
    get().updateDraft((workflow) => ({
      ...workflow,
      nodes: workflow.nodes.length > 1 ? workflow.nodes.filter((node) => node.id !== nodeId) : workflow.nodes,
    }));
  },

  moveNode(nodeId, direction) {
    get().updateDraft((workflow) => {
      const index = workflow.nodes.findIndex((node) => node.id === nodeId);
      const destination = index + direction;
      if (index < 0 || destination < 0 || destination >= workflow.nodes.length) return workflow;
      const nodes = [...workflow.nodes];
      [nodes[index], nodes[destination]] = [nodes[destination]!, nodes[index]!];
      return { ...workflow, nodes };
    });
  },

  async save(port) {
    const ownerUserId = get().ownerUserId;
    const draft = get().draft;
    if (!ownerUserId || !draft) return false;
    const parsed = mediaWorkflowSchema.safeParse(draft);
    if (!parsed.success) {
      set({ errorMessage: parsed.error.issues[0]?.message ?? "工作流内容无效。" });
      return false;
    }
    const saveRequestId = crypto.randomUUID();
    set({ isSaving: true, errorMessage: null, activeSaveRequestId: saveRequestId });
    try {
      const saved = await port.saveWorkflow(ownerUserId, parsed.data);
      if (get().activeSaveRequestId !== saveRequestId || get().ownerUserId !== ownerUserId) return saved;
      if (!saved) {
        set({ isSaving: false, activeSaveRequestId: null, errorMessage: "工作流已在其他窗口更新，请重新打开后再保存。" });
        return false;
      }
      const workflows = [parsed.data, ...get().workflows.filter((workflow) => workflow.id !== parsed.data.id)]
        .sort((left, right) => right.updatedAtUnixMs - left.updatedAtUnixMs);
      const currentDraft = get().draft;
      set({
        workflows,
        draft: currentDraft?.id === parsed.data.id
          && currentDraft.updatedAtUnixMs === parsed.data.updatedAtUnixMs
          ? structuredClone(parsed.data)
          : currentDraft,
        isSaving: false,
        activeSaveRequestId: null,
      });
      return true;
    } catch (error) {
      if (get().activeSaveRequestId === saveRequestId && get().ownerUserId === ownerUserId) {
        set({ isSaving: false, activeSaveRequestId: null, errorMessage: errorMessage(error) });
      }
      return false;
    }
  },

  async remove(port, workflowId = get().selectedWorkflowId ?? undefined) {
    const ownerUserId = get().ownerUserId;
    const id = workflowId;
    if (!ownerUserId || !id || get().isRunning) return;
    set({ isSaving: true, errorMessage: null });
    try {
      await port.deleteWorkflow(ownerUserId, id);
      set({
        workflows: get().workflows.filter((workflow) => workflow.id !== id),
        selectedWorkflowId: null,
        draft: null,
        isSaving: false,
      });
    } catch (error) {
      set({ isSaving: false, errorMessage: errorMessage(error) });
    }
  },

  async run(port) {
    const ownerUserId = get().ownerUserId;
    const draft = get().draft;
    if (!ownerUserId || !draft || get().isRunning || get().isSaving) return;
    const parsed = mediaWorkflowSchema.safeParse(draft);
    if (!parsed.success) {
      set({ errorMessage: parsed.error.issues[0]?.message ?? "工作流内容无效。" });
      return;
    }
    const incompatibleModel = incompatibleWorkflowModelMessage(parsed.data);
    if (incompatibleModel) {
      set({ errorMessage: incompatibleModel });
      return;
    }
    const saved = get().workflows.find((workflow) => workflow.id === parsed.data.id);
    if (!saved || saved.updatedAtUnixMs !== parsed.data.updatedAtUnixMs) {
      if (!await get().save(port) || !get().draft) return;
    }
    const clientRequestId = crypto.randomUUID();
    const scope = { ownerUserId, workflowId: parsed.data.id, clientRequestId };
    set({ isRunning: true, errorMessage: null, runResult: null, nodeStates: {}, activeRunRequestId: clientRequestId });
    let unlisten: (() => void) | undefined;
    try {
      unlisten = await port.watchWorkflowRun(scope, (event) => applyRunEvent(set, get, scope, event));
      const result = await port.runWorkflow(scope);
      const resultsByNode = new Map(result.nodeResults.map((node) => [node.nodeId, node.result]));
      if (result.workflowId !== scope.workflowId
        || result.clientRequestId !== scope.clientRequestId
        || resultsByNode.size !== parsed.data.nodes.length
        || parsed.data.nodes.some((node) => {
          const nodeResult = resultsByNode.get(node.id);
          return nodeResult == null || !resultMatchesNodeKind(node.kind, nodeResult.type);
        })) {
        throw new Error("工作流运行结果与当前请求不匹配。");
      }
      if (!isCurrentRun(get(), scope)) return;
      mergeNodeResultsIntoHistory(scope.ownerUserId, result.nodeResults.map((node) => node.result));
      const nodeStates = { ...get().nodeStates };
      parsed.data.nodes.forEach((node) => {
        nodeStates[node.id] = { phase: "completed", result: resultsByNode.get(node.id)! };
      });
      set({ isRunning: false, runResult: result, nodeStates, activeRunRequestId: null });
    } catch (error) {
      if (!isCurrentRun(get(), scope)) return;
      const message = errorMessage(error);
      const nodeStates = { ...get().nodeStates };
      const runningNodeId = Object.entries(nodeStates).find(([, state]) => state.phase === "running")?.[0];
      if (runningNodeId) nodeStates[runningNodeId] = { phase: "failed", errorMessage: message };
      set({ isRunning: false, errorMessage: message, nodeStates, activeRunRequestId: null });
    } finally {
      unlisten?.();
    }
  },

  dismissError() { set({ errorMessage: null }); },
  reset() {
    set({ ownerUserId: null, workflows: [], selectedWorkflowId: null, draft: null, isLoading: false, isSaving: false, isRunning: false, nodeStates: {}, runResult: null, activeRunRequestId: null, activeSaveRequestId: null, errorMessage: null });
  },
}));

function isCurrentRun(
  state: MediaWorkflowState,
  scope: { ownerUserId: string; workflowId: string; clientRequestId: string },
) {
  return state.ownerUserId === scope.ownerUserId
    && state.selectedWorkflowId === scope.workflowId
    && state.activeRunRequestId === scope.clientRequestId;
}

function resultMatchesNodeKind(
  kind: MediaWorkflowNodeKind,
  resultType: MediaWorkflowNodeRunResult["type"],
) {
  return kind === "text" ? resultType === "text" : kind === "image" ? resultType === "images" : resultType === "video";
}

function applyRunEvent(
  set: (partial: Partial<MediaWorkflowState>) => void,
  get: () => MediaWorkflowState,
  scope: { ownerUserId: string; workflowId: string; clientRequestId: string },
  event: MediaWorkflowRunEvent,
) {
  if (!isCurrentRun(get(), scope)) return;
  const nodeState: MediaWorkflowNodeState = event.phase === "running"
    ? { phase: "running", progress: event.progress }
    : event.phase === "completed"
      ? { phase: "completed", result: event.result }
      : { phase: "failed", errorMessage: event.errorMessage };
  if (event.phase === "completed") mergeNodeResultsIntoHistory(scope.ownerUserId, [event.result]);
  set({ nodeStates: { ...get().nodeStates, [event.nodeId]: nodeState } });
}

function mergeNodeResultsIntoHistory(ownerUserId: string, results: MediaWorkflowNodeRunResult[]) {
  useMediaStudioStore.getState().mergeWorkflowResults(
    ownerUserId,
    results.flatMap((result) => result.type === "images" ? [result.item] : []),
    results.flatMap((result) => result.type === "video" ? [result.item] : []),
  );
}

function incompatibleWorkflowModelMessage(workflow: MediaWorkflow) {
  const media = useMediaStudioStore.getState();
  let hasImages = false;
  for (const node of workflow.nodes) {
    if (node.kind === "text") {
      const model = media.textModels.find((candidate) => candidate.id === node.modelConfigId);
      if (!model) return "文本节点所选模型不可用或不支持文本生成。";
      if ((hasImages || node.referenceImages.length > 0) && !model.supportsImageInput) {
        return "文本节点所选模型不支持读取当前图片输入。";
      }
    } else if (node.kind === "image") {
      if (!media.imageModels.some((candidate) => candidate.id === node.modelConfigId)) {
        return "图片节点所选模型不可用或不支持图片生成。";
      }
      hasImages = true;
    } else if (!media.videoModels.some((candidate) => candidate.id === node.modelConfigId)) {
      return "视频节点所选模型不可用或不支持视频生成。";
    }
  }
  return null;
}
