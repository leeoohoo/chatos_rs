import { create } from "zustand";
import {
  mediaWorkflowSchema,
  newMediaWorkflow,
  newMediaWorkflowNode,
  type MediaWorkflow,
  type MediaWorkflowNodeKind,
} from "../domain/mediaWorkflow";
import type { MediaStudioPort } from "../ports/MediaStudioPort";

interface MediaWorkflowState {
  ownerUserId: string | null;
  workflows: MediaWorkflow[];
  selectedWorkflowId: string | null;
  draft: MediaWorkflow | null;
  isLoading: boolean;
  isSaving: boolean;
  isRunning: boolean;
  runResult: import("../domain/mediaWorkflow").MediaWorkflowRunResult | null;
  errorMessage: string | null;
  activate(port: MediaStudioPort, ownerUserId: string): Promise<void>;
  create(modelConfigId?: string): void;
  open(workflow: MediaWorkflow): void;
  close(): void;
  updateDraft(update: (workflow: MediaWorkflow) => MediaWorkflow): void;
  addNode(kind: MediaWorkflowNodeKind, modelConfigId?: string): void;
  removeNode(nodeId: string): void;
  moveNode(nodeId: string, direction: -1 | 1): void;
  save(port: MediaStudioPort): Promise<void>;
  run(port: MediaStudioPort): Promise<void>;
  remove(port: MediaStudioPort, workflowId?: string): Promise<void>;
  dismissError(): void;
  reset(): void;
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "工作流操作失败。";
}

export const useMediaWorkflowStore = create<MediaWorkflowState>((set, get) => ({
  ownerUserId: null,
  workflows: [],
  selectedWorkflowId: null,
  draft: null,
  isLoading: false,
  isSaving: false,
  isRunning: false,
  runResult: null,
  errorMessage: null,

  async activate(port, ownerInput) {
    const ownerUserId = ownerInput.trim();
    if (!ownerUserId) {
      get().reset();
      return;
    }
    if (get().ownerUserId === ownerUserId && !get().isLoading) return;
    set({ ownerUserId, isLoading: true, errorMessage: null, selectedWorkflowId: null, draft: null });
    try {
      const workflows = await port.listWorkflows(ownerUserId);
      if (get().ownerUserId !== ownerUserId) return;
      set({ workflows, isLoading: false });
    } catch (error) {
      if (get().ownerUserId !== ownerUserId) return;
      set({ workflows: [], isLoading: false, errorMessage: errorMessage(error) });
    }
  },

  create(modelConfigId = "") {
    const existing = get().workflows.length;
    const workflow = newMediaWorkflow(modelConfigId);
    workflow.name = `工作流 ${existing + 1}`;
    set({ selectedWorkflowId: workflow.id, draft: workflow, errorMessage: null, runResult: null });
  },

  open(workflow) {
    const parsed = mediaWorkflowSchema.parse(workflow);
    set({ selectedWorkflowId: parsed.id, draft: structuredClone(parsed), errorMessage: null, runResult: null });
  },

  close() {
    if (get().isSaving) return;
    set({ selectedWorkflowId: null, draft: null, errorMessage: null, runResult: null });
  },

  updateDraft(update) {
    const current = get().draft;
    if (!current) return;
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
    if (!ownerUserId || !draft) return;
    const parsed = mediaWorkflowSchema.safeParse(draft);
    if (!parsed.success) {
      set({ errorMessage: parsed.error.issues[0]?.message ?? "工作流内容无效。" });
      return;
    }
    set({ isSaving: true, errorMessage: null });
    try {
      await port.saveWorkflow(ownerUserId, parsed.data);
      const workflows = [parsed.data, ...get().workflows.filter((workflow) => workflow.id !== parsed.data.id)]
        .sort((left, right) => right.updatedAtUnixMs - left.updatedAtUnixMs);
      set({ workflows, draft: structuredClone(parsed.data), isSaving: false });
    } catch (error) {
      set({ isSaving: false, errorMessage: errorMessage(error) });
    }
  },

  async remove(port, workflowId = get().selectedWorkflowId ?? undefined) {
    const ownerUserId = get().ownerUserId;
    const id = workflowId;
    if (!ownerUserId || !id) return;
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
    if (!ownerUserId || !draft || get().isRunning) return;
    const parsed = mediaWorkflowSchema.safeParse(draft);
    if (!parsed.success) {
      set({ errorMessage: parsed.error.issues[0]?.message ?? "工作流内容无效。" });
      return;
    }
    const saved = get().workflows.find((workflow) => workflow.id === parsed.data.id);
    if (!saved || saved.updatedAtUnixMs !== parsed.data.updatedAtUnixMs) {
      await get().save(port);
      if (get().errorMessage || !get().draft) return;
    }
    set({ isRunning: true, errorMessage: null, runResult: null });
    try {
      const result = await port.runWorkflow({
        ownerUserId,
        workflowId: parsed.data.id,
        clientRequestId: crypto.randomUUID(),
      });
      set({ isRunning: false, runResult: result });
    } catch (error) {
      set({ isRunning: false, errorMessage: errorMessage(error) });
    }
  },

  dismissError() { set({ errorMessage: null }); },
  reset() {
    set({ ownerUserId: null, workflows: [], selectedWorkflowId: null, draft: null, isLoading: false, isSaving: false, isRunning: false, runResult: null, errorMessage: null });
  },
}));
