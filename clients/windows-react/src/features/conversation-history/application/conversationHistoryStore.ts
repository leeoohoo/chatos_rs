import { create } from "zustand";
import {
  idleConversationHistory,
  mergeOlderHistory,
  type ConversationHistoryPhase,
} from "../domain/conversationHistory";
import type { ConversationHistoryPort } from "../ports/ConversationHistoryPort";

interface ConversationHistoryState {
  ownerUserId: string | null;
  conversationId: string | null;
  requestSequence: number;
  requestId: number;
  phase: ConversationHistoryPhase;
  load(port: ConversationHistoryPort, ownerUserId: string, conversationId: string, force?: boolean): Promise<void>;
  loadOlder(port: ConversationHistoryPort): Promise<void>;
  reset(ownerUserId?: string): void;
}

export const useConversationHistoryStore = create<ConversationHistoryState>((set, get) => ({
  ownerUserId: null,
  conversationId: null,
  requestSequence: 0,
  requestId: 0,
  phase: idleConversationHistory,

  async load(port, ownerInput, conversationInput, force = false) {
    const ownerUserId = ownerInput.trim();
    const conversationId = conversationInput.trim();
    if (!ownerUserId || !conversationId) return;
    const current = get();
    if (current.ownerUserId !== ownerUserId || current.conversationId !== conversationId) {
      get().reset(ownerUserId);
    }
    const phase = get().phase;
    if (phase.kind === "loading" || (!force && ["ready", "failed"].includes(phase.kind))) return;
    const requestId = get().requestSequence + 1;
    set({ ownerUserId, conversationId, requestSequence: requestId, requestId, phase: { kind: "loading" } });
    try {
      const snapshot = await port.loadHistory(ownerUserId, conversationId);
      if (!accepts(get, ownerUserId, conversationId, requestId)) return;
      if (snapshot.conversation.owner_user_id !== ownerUserId
        || snapshot.conversation.conversation_id !== conversationId) {
        throw new Error("Host 返回了错误的会话历史。");
      }
      set({ phase: { kind: "ready", snapshot, isLoadingOlder: false, olderError: null } });
    } catch (error) {
      if (!accepts(get, ownerUserId, conversationId, requestId)) return;
      set({ phase: { kind: "failed", message: errorMessage(error) } });
    }
  },

  async loadOlder(port) {
    const current = get();
    if (!current.ownerUserId || !current.conversationId || current.phase.kind !== "ready") return;
    const beforeOrdinal = current.phase.snapshot.next_before_ordinal;
    if (current.phase.isLoadingOlder || beforeOrdinal == null) return;
    const requestId = current.requestSequence + 1;
    const snapshot = current.phase.snapshot;
    set({
      requestSequence: requestId,
      requestId,
      phase: { ...current.phase, isLoadingOlder: true, olderError: null },
    });
    try {
      const older = await port.loadHistory(
        current.ownerUserId,
        current.conversationId,
        beforeOrdinal,
      );
      if (!accepts(get, current.ownerUserId, current.conversationId, requestId)) return;
      set({
        phase: {
          kind: "ready",
          snapshot: mergeOlderHistory(snapshot, older),
          isLoadingOlder: false,
          olderError: null,
        },
      });
    } catch (error) {
      if (!accepts(get, current.ownerUserId, current.conversationId, requestId)) return;
      set({
        phase: {
          kind: "ready",
          snapshot,
          isLoadingOlder: false,
          olderError: errorMessage(error),
        },
      });
    }
  },

  reset(ownerUserId = "") {
    set((state) => ({
      ownerUserId: ownerUserId.trim() || null,
      conversationId: null,
      requestSequence: state.requestSequence + 1,
      requestId: 0,
      phase: idleConversationHistory,
    }));
  },
}));

function accepts(
  get: () => ConversationHistoryState,
  ownerUserId: string,
  conversationId: string,
  requestId: number,
) {
  const state = get();
  return state.ownerUserId === ownerUserId
    && state.conversationId === conversationId
    && state.requestId === requestId;
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "会话历史加载失败。";
}
