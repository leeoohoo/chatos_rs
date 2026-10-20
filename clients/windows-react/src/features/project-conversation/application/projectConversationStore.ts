import { create } from "zustand";
import {
  idleProjectConversation,
  type ProjectConversationPhase,
} from "../domain/projectConversation";
import type { ProjectConversationPort } from "../ports/ProjectConversationPort";

interface ProjectConversationEntry {
  requestId: number;
  phase: ProjectConversationPhase;
}

interface PrepareProjectConversationInput {
  ownerUserId: string;
  projectId: string;
  existingConversationId: string | null;
  hostReady: boolean;
  defaultContactReady: boolean;
  force?: boolean;
}

interface ProjectConversationState {
  ownerUserId: string | null;
  requestSequence: number;
  entries: Record<string, ProjectConversationEntry>;
  phase(projectId: string): ProjectConversationPhase;
  prepare(port: ProjectConversationPort, input: PrepareProjectConversationInput): Promise<void>;
  remove(projectId: string): void;
  reset(ownerUserId?: string): void;
}

export const useProjectConversationStore = create<ProjectConversationState>((set, get) => ({
  ownerUserId: null,
  requestSequence: 0,
  entries: {},

  phase(projectId) {
    return get().entries[projectId]?.phase ?? idleProjectConversation;
  },

  async prepare(port, input) {
    const ownerUserId = input.ownerUserId.trim();
    const projectId = input.projectId.trim();
    if (!ownerUserId || !projectId) return;
    if (get().ownerUserId !== ownerUserId) get().reset(ownerUserId);
    const current = get().entries[projectId];
    if (input.existingConversationId) {
      setEntry(set, get, projectId, {
        requestId: current?.requestId ?? 0,
        phase: { kind: "available", conversationId: input.existingConversationId },
      });
      return;
    }
    if (!input.hostReady || !input.defaultContactReady) return;
    if (current?.phase.kind === "preparing") return;
    if (!input.force && current?.phase.kind === "failed") return;

    const requestId = get().requestSequence + 1;
    set((state) => ({
      requestSequence: requestId,
      entries: {
        ...state.entries,
        [projectId]: { requestId, phase: { kind: "preparing" } },
      },
    }));
    try {
      const prepared = await port.prepareProjectConversation(ownerUserId, projectId);
      if (!accepts(get, ownerUserId, projectId, requestId)) return;
      if (prepared.projectId !== projectId) {
        throw new Error("Host 返回了错误的项目会话绑定。");
      }
      setEntry(set, get, projectId, {
        requestId,
        phase: { kind: "available", conversationId: prepared.conversationId },
      });
    } catch (error) {
      if (!accepts(get, ownerUserId, projectId, requestId)) return;
      setEntry(set, get, projectId, {
        requestId,
        phase: { kind: "failed", message: errorMessage(error) },
      });
    }
  },

  remove(projectInput) {
    const projectId = projectInput.trim();
    if (!projectId || !get().entries[projectId]) return;
    set((state) => {
      const entries = { ...state.entries };
      delete entries[projectId];
      return { entries, requestSequence: state.requestSequence + 1 };
    });
  },

  reset(ownerUserId = "") {
    set((state) => ({
      ownerUserId: ownerUserId.trim() || null,
      requestSequence: state.requestSequence + 1,
      entries: {},
    }));
  },
}));

function accepts(
  get: () => ProjectConversationState,
  ownerUserId: string,
  projectId: string,
  requestId: number,
) {
  return get().ownerUserId === ownerUserId && get().entries[projectId]?.requestId === requestId;
}

function setEntry(
  set: (
    partial:
      | Partial<ProjectConversationState>
      | ((state: ProjectConversationState) => Partial<ProjectConversationState>),
  ) => void,
  get: () => ProjectConversationState,
  projectId: string,
  entry: ProjectConversationEntry,
) {
  if (get().entries[projectId]?.requestId !== entry.requestId && entry.requestId !== 0) return;
  set((state) => ({ entries: { ...state.entries, [projectId]: entry } }));
}

function errorMessage(error: unknown): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "项目会话准备失败。";
}
