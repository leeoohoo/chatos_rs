import { create } from "zustand";
import type { ConversationHistorySnapshot } from "../../conversation-history/domain/conversationHistory";
import {
  planConversationSend,
  recoverComposerDraft,
  type ExplicitCapabilitySelection,
} from "../domain/conversationComposer";
import type { ConversationComposerPort } from "../ports/ConversationComposerPort";

interface ConversationComposerState {
  ownerUserId: string | null;
  conversationId: string | null;
  draft: string;
  explicitCapabilitySelection: ExplicitCapabilitySelection | null;
  isSending: boolean;
  error: string | null;
  requestSequence: number;
  requestId: number;
  setDraft(value: string): void;
  clearError(): void;
  send(
    port: ConversationComposerPort,
    ownerUserId: string,
    conversationId: string,
    history: ConversationHistorySnapshot,
    onAccepted: () => Promise<void>,
  ): Promise<void>;
  reset(ownerUserId?: string, conversationId?: string): void;
}

export const useConversationComposerStore = create<ConversationComposerState>((set, get) => ({
  ownerUserId: null,
  conversationId: null,
  draft: "",
  explicitCapabilitySelection: null,
  isSending: false,
  error: null,
  requestSequence: 0,
  requestId: 0,

  setDraft(value) {
    set({ draft: value, error: null });
  },

  clearError() {
    set({ error: null });
  },

  async send(port, ownerInput, conversationInput, history, onAccepted) {
    const ownerUserId = ownerInput.trim();
    const conversationId = conversationInput.trim();
    if (!ownerUserId || !conversationId) return;
    if (get().ownerUserId !== ownerUserId || get().conversationId !== conversationId) {
      get().reset(ownerUserId, conversationId);
    }
    const current = get();
    const plan = planConversationSend(
      current.draft,
      current.explicitCapabilitySelection,
      history,
      current.isSending,
    );
    if (plan.kind === "ignore") return;
    if (plan.kind === "blocked") {
      set({ error: plan.message });
      return;
    }
    const requestId = current.requestSequence + 1;
    const submission = plan.submission;
    set({
      ownerUserId,
      conversationId,
      draft: "",
      explicitCapabilitySelection: null,
      isSending: true,
      error: null,
      requestSequence: requestId,
      requestId,
    });
    try {
      await port.sendDraft({
        ownerUserId,
        conversationId,
        guidanceTurnId: plan.kind === "guidance" ? plan.turnId : null,
        submission,
      });
      if (!accepts(get, ownerUserId, conversationId, requestId)) return;
      await onAccepted();
      if (!accepts(get, ownerUserId, conversationId, requestId)) return;
      set({ isSending: false, requestId: 0 });
    } catch (error) {
      if (!accepts(get, ownerUserId, conversationId, requestId)) return;
      set((state) => ({
        draft: recoverComposerDraft(state.draft, submission.content),
        explicitCapabilitySelection:
          state.explicitCapabilitySelection ?? submission.explicitCapabilitySelection,
        isSending: false,
        error: errorMessage(error),
        requestId: 0,
      }));
    }
  },

  reset(ownerUserId = "", conversationId = "") {
    set((state) => ({
      ownerUserId: ownerUserId.trim() || null,
      conversationId: conversationId.trim() || null,
      draft: "",
      explicitCapabilitySelection: null,
      isSending: false,
      error: null,
      requestSequence: state.requestSequence + 1,
      requestId: 0,
    }));
  },
}));

function accepts(
  get: () => ConversationComposerState,
  ownerUserId: string,
  conversationId: string,
  requestId: number,
) {
  const state = get();
  return state.ownerUserId === ownerUserId
    && state.conversationId === conversationId
    && state.requestId === requestId;
}

function errorMessage(error: unknown): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "消息发送失败，请重试。";
}
