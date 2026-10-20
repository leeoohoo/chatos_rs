import { create } from "zustand";
import type { ConversationRealtimeSnapshot } from "../domain/conversationRealtime";
import type { ConversationRealtimePort } from "../ports/ConversationRealtimePort";

interface ConversationRealtimeState {
  ownerUserId: string | null;
  conversationId: string | null;
  requestSequence: number;
  requestId: number;
  snapshot: ConversationRealtimeSnapshot;
  activate(ownerUserId: string, conversationId: string, conversationVersion: number): void;
  poll(port: ConversationRealtimePort, onAffected: () => Promise<void>): Promise<void>;
  reset(): void;
}

const idleSnapshot: ConversationRealtimeSnapshot = {
  cursor: null,
  conversationVersion: 0,
  error: null,
};

export const useConversationRealtimeStore = create<ConversationRealtimeState>((set, get) => ({
  ownerUserId: null,
  conversationId: null,
  requestSequence: 0,
  requestId: 0,
  snapshot: idleSnapshot,

  activate(ownerInput, conversationInput, conversationVersion) {
    const ownerUserId = ownerInput.trim();
    const conversationId = conversationInput.trim();
    const current = get();
    if (current.ownerUserId === ownerUserId && current.conversationId === conversationId) {
      if (conversationVersion > current.snapshot.conversationVersion) {
        set({ snapshot: { ...current.snapshot, conversationVersion } });
      }
      return;
    }
    set((state) => ({
      ownerUserId: ownerUserId || null,
      conversationId: conversationId || null,
      requestSequence: state.requestSequence + 1,
      requestId: 0,
      snapshot: { ...idleSnapshot, conversationVersion },
    }));
  },

  async poll(port, onAffected) {
    const current = get();
    if (!current.ownerUserId || !current.conversationId || current.requestId !== 0) return;
    const requestId = current.requestSequence + 1;
    const ownerUserId = current.ownerUserId;
    const conversationId = current.conversationId;
    set({ requestSequence: requestId, requestId });
    try {
      const result = await port.poll({
        ownerUserId,
        conversationId,
        afterCursor: current.snapshot.cursor,
        observedConversationVersion: current.snapshot.conversationVersion,
      });
      if (!accepts(get, ownerUserId, conversationId, requestId)) return;
      set({
        requestId: 0,
        snapshot: {
          cursor: result.nextCursor,
          conversationVersion: result.conversationVersion,
          error: null,
        },
      });
      if (result.affected) await onAffected();
    } catch (error) {
      if (!accepts(get, ownerUserId, conversationId, requestId)) return;
      set((state) => ({
        requestId: 0,
        snapshot: { ...state.snapshot, error: errorMessage(error) },
      }));
    }
  },

  reset() {
    set((state) => ({
      ownerUserId: null,
      conversationId: null,
      requestSequence: state.requestSequence + 1,
      requestId: 0,
      snapshot: idleSnapshot,
    }));
  },
}));

function accepts(
  get: () => ConversationRealtimeState,
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
  return "会话实时更新暂时不可用。";
}
