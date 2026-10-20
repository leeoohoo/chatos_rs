import { create } from "zustand";
import {
  mergeAttachmentDrafts,
  type AttachmentDragState,
  type AttachmentImportResult,
  type AttachmentScope,
  type ConversationAttachmentDraft,
} from "../../conversation-attachments/domain/conversationAttachment";
import type { ConversationAttachmentPort } from "../../conversation-attachments/ports/ConversationAttachmentPort";
import type { ConversationHistorySnapshot } from "../../conversation-history/domain/conversationHistory";
import {
  AVAILABLE_COMPOSER_CAPABILITIES,
  filterCapabilityCandidates,
  planConversationSend,
  recoverComposerDraft,
  removeTrailingCapabilityQuery,
  trailingCapabilityQuery,
  type ComposerCapabilityCandidate,
  type ExplicitCapabilitySelection,
} from "../domain/conversationComposer";
import type { ConversationComposerPort } from "../ports/ConversationComposerPort";

interface ConversationComposerState {
  ownerUserId: string | null;
  conversationId: string | null;
  draft: string;
  explicitCapabilitySelection: ExplicitCapabilitySelection | null;
  attachments: ConversationAttachmentDraft[];
  attachmentError: string | null;
  isImportingAttachments: boolean;
  isAttachmentDropTargeted: boolean;
  attachmentDraftGeneration: number;
  attachmentImportCount: number;
  capabilitySuggestionsSuppressed: boolean;
  highlightedCapabilityId: string | null;
  isSending: boolean;
  error: string | null;
  requestSequence: number;
  requestId: number;
  setDraft(
    value: string,
    candidates?: readonly ComposerCapabilityCandidate[],
  ): void;
  setExplicitCapabilitySelection(value: ExplicitCapabilitySelection | null): void;
  dismissCapabilitySuggestions(): void;
  moveCapabilityHighlight(
    direction: "up" | "down",
    candidates: readonly ComposerCapabilityCandidate[],
  ): void;
  selectCapability(candidate: ComposerCapabilityCandidate): void;
  activateAttachmentScope(
    port: ConversationAttachmentPort,
    ownerUserId: string,
    conversationId: string,
  ): Promise<void>;
  pickAttachments(
    port: ConversationAttachmentPort,
    ownerUserId: string,
    conversationId: string,
  ): Promise<void>;
  importClipboard(
    port: ConversationAttachmentPort,
    ownerUserId: string,
    conversationId: string,
  ): Promise<string | null>;
  acceptAttachmentImport(
    port: ConversationAttachmentPort,
    result: AttachmentImportResult,
  ): void;
  setAttachmentDragState(state: AttachmentDragState): void;
  removeAttachment(port: ConversationAttachmentPort, attachmentId: string): Promise<void>;
  clearAttachmentError(): void;
  clearError(): void;
  send(
    port: ConversationComposerPort,
    ownerUserId: string,
    conversationId: string,
    history: ConversationHistorySnapshot,
    attachmentPort: ConversationAttachmentPort,
    onAccepted: () => Promise<void>,
  ): Promise<void>;
  reset(
    ownerUserId?: string,
    conversationId?: string,
    attachmentPort?: ConversationAttachmentPort,
  ): void;
}

export const useConversationComposerStore = create<ConversationComposerState>((set, get) => ({
  ownerUserId: null,
  conversationId: null,
  draft: "",
  explicitCapabilitySelection: null,
  attachments: [],
  attachmentError: null,
  isImportingAttachments: false,
  isAttachmentDropTargeted: false,
  attachmentDraftGeneration: 0,
  attachmentImportCount: 0,
  capabilitySuggestionsSuppressed: false,
  highlightedCapabilityId: null,
  isSending: false,
  error: null,
  requestSequence: 0,
  requestId: 0,

  setDraft(value, candidates = AVAILABLE_COMPOSER_CAPABILITIES) {
    const query = trailingCapabilityQuery(value);
    const visible = query ? filterCapabilityCandidates(candidates, query).slice(0, 8) : [];
    set({
      draft: value,
      error: null,
      capabilitySuggestionsSuppressed: false,
      highlightedCapabilityId: visible[0]?.id ?? null,
    });
  },

  setExplicitCapabilitySelection(value) {
    set({ explicitCapabilitySelection: value, error: null });
  },

  dismissCapabilitySuggestions() {
    set({ capabilitySuggestionsSuppressed: true, highlightedCapabilityId: null });
  },

  moveCapabilityHighlight(direction, candidates) {
    const current = get();
    if (current.capabilitySuggestionsSuppressed) return;
    const query = trailingCapabilityQuery(current.draft);
    if (!query) return;
    const visible = filterCapabilityCandidates(candidates, query).slice(0, 8);
    if (visible.length === 0) return;
    const currentIndex = visible.findIndex(({ id }) => id === current.highlightedCapabilityId);
    const index = currentIndex < 0 ? 0 : currentIndex;
    const offset = direction === "down" ? 1 : -1;
    set({ highlightedCapabilityId: visible[(index + offset + visible.length) % visible.length]!.id });
  },

  selectCapability(candidate) {
    const current = get();
    const query = trailingCapabilityQuery(current.draft);
    if (!query) return;
    set({
      draft: removeTrailingCapabilityQuery(current.draft, query),
      explicitCapabilitySelection: candidate.selection,
      capabilitySuggestionsSuppressed: false,
      highlightedCapabilityId: null,
      error: null,
    });
  },

  async activateAttachmentScope(port, ownerInput, conversationInput) {
    const scope = attachmentScope(get(), ownerInput, conversationInput);
    if (!scope) return;
    try {
      await port.activateScope(scope);
      if (!acceptsAttachmentScope(get(), scope)) return;
      set({ attachmentError: null });
    } catch (error) {
      if (!acceptsAttachmentScope(get(), scope)) return;
      set({ attachmentError: errorMessage(error) });
    }
  },

  async pickAttachments(port, ownerInput, conversationInput) {
    const scope = attachmentScope(get(), ownerInput, conversationInput);
    if (!scope) return;
    beginAttachmentImport(set);
    try {
      const result = await port.pickFiles(scope);
      finishAttachmentImport(set);
      applyAttachmentImport(get, set, port, result);
    } catch (error) {
      finishAttachmentImport(set);
      if (acceptsAttachmentScope(get(), scope)) {
        set({ attachmentError: errorMessage(error) });
      }
    }
  },

  async importClipboard(port, ownerInput, conversationInput) {
    const scope = attachmentScope(get(), ownerInput, conversationInput);
    if (!scope) return null;
    beginAttachmentImport(set);
    try {
      const response = await port.importClipboard(scope);
      finishAttachmentImport(set);
      if (!acceptsAttachmentScope(get(), scope)) {
        if (response.kind === "attachments") discardImport(port, response.result);
        return null;
      }
      if (response.kind === "attachments") {
        applyAttachmentImport(get, set, port, response.result);
        return null;
      }
      return response.kind === "text" ? response.text : null;
    } catch (error) {
      finishAttachmentImport(set);
      if (acceptsAttachmentScope(get(), scope)) {
        set({ attachmentError: errorMessage(error) });
      }
      return null;
    }
  },

  acceptAttachmentImport(port, result) {
    applyAttachmentImport(get, set, port, result);
  },

  setAttachmentDragState(state) {
    const current = get();
    const matches = state.scope != null && acceptsAttachmentScope(get(), state.scope);
    if (!matches && state.targeted) return;
    if (current.isAttachmentDropTargeted !== (matches && state.targeted)) {
      set({ isAttachmentDropTargeted: matches && state.targeted });
    }
  },

  async removeAttachment(port, attachmentId) {
    const current = get();
    const scope = attachmentScope(current, current.ownerUserId ?? "", current.conversationId ?? "");
    if (!scope || !current.attachments.some(({ id }) => id === attachmentId)) return;
    try {
      await port.discard(scope, [attachmentId]);
      if (!acceptsAttachmentScope(get(), scope)) return;
      set((state) => ({
        attachments: state.attachments.filter(({ id }) => id !== attachmentId),
        attachmentError: null,
      }));
    } catch (error) {
      if (acceptsAttachmentScope(get(), scope)) {
        set({ attachmentError: errorMessage(error) });
      }
    }
  },

  clearAttachmentError() {
    set({ attachmentError: null });
  },

  clearError() {
    set({ error: null });
  },

  async send(port, ownerInput, conversationInput, history, attachmentPort, onAccepted) {
    const ownerUserId = ownerInput.trim();
    const conversationId = conversationInput.trim();
    if (!ownerUserId || !conversationId) return;
    if (get().ownerUserId !== ownerUserId || get().conversationId !== conversationId) {
      get().reset(ownerUserId, conversationId);
    }
    const current = get();
    const plan = planConversationSend(
      current.draft,
      current.attachments.map(({ id }) => id),
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
    const submittedAttachments = current.attachments;
    const submittedAttachmentScope: AttachmentScope = {
      ownerUserId,
      conversationId,
      draftGeneration: current.attachmentDraftGeneration,
    };
    const nextAttachmentGeneration = current.attachmentDraftGeneration + 1;
    set({
      ownerUserId,
      conversationId,
      draft: "",
      explicitCapabilitySelection: null,
      attachments: [],
      attachmentError: null,
      isImportingAttachments: false,
      isAttachmentDropTargeted: false,
      attachmentDraftGeneration: nextAttachmentGeneration,
      attachmentImportCount: 0,
      capabilitySuggestionsSuppressed: false,
      highlightedCapabilityId: null,
      isSending: true,
      error: null,
      requestSequence: requestId,
      requestId,
    });
    void attachmentPort.activateScope({
      ownerUserId,
      conversationId,
      draftGeneration: nextAttachmentGeneration,
    });
    try {
      await port.sendDraft({
        ownerUserId,
        conversationId,
        guidanceTurnId: plan.kind === "guidance" ? plan.turnId : null,
        submission,
      });
      if (!accepts(get, ownerUserId, conversationId, requestId)) return;
    } catch (error) {
      if (!accepts(get, ownerUserId, conversationId, requestId)) {
        if (submittedAttachments.length) {
          void attachmentPort.discard(
            submittedAttachmentScope,
            submittedAttachments.map(({ id }) => id),
          );
        }
        return;
      }
      set((state) => ({
        draft: recoverComposerDraft(state.draft, submission.content),
        explicitCapabilitySelection:
          state.explicitCapabilitySelection ?? submission.explicitCapabilitySelection,
        attachments: mergeAttachmentDrafts(submittedAttachments, state.attachments),
        isSending: false,
        error: errorMessage(error),
        requestId: 0,
      }));
      return;
    }
    if (!accepts(get, ownerUserId, conversationId, requestId)) return;
    set({ isSending: false, requestId: 0 });
    try {
      await onAccepted();
    } catch {
      // The Host already accepted the message. History refresh owns its own error state.
    }
  },

  reset(ownerUserId = "", conversationId = "", attachmentPort) {
    const current = get();
    if (attachmentPort && current.ownerUserId && current.conversationId && current.attachments.length) {
      void attachmentPort.discard({
        ownerUserId: current.ownerUserId,
        conversationId: current.conversationId,
        draftGeneration: current.attachmentDraftGeneration,
      }, current.attachments.map(({ id }) => id));
    }
    set((state) => ({
      ownerUserId: ownerUserId.trim() || null,
      conversationId: conversationId.trim() || null,
      draft: "",
      explicitCapabilitySelection: null,
      attachments: [],
      attachmentError: null,
      isImportingAttachments: false,
      isAttachmentDropTargeted: false,
      attachmentDraftGeneration: state.attachmentDraftGeneration + 1,
      attachmentImportCount: 0,
      capabilitySuggestionsSuppressed: false,
      highlightedCapabilityId: null,
      isSending: false,
      error: null,
      requestSequence: state.requestSequence + 1,
      requestId: 0,
    }));
  },
}));

type ComposerSet = (
  partial: Partial<ConversationComposerState>
    | ((state: ConversationComposerState) => Partial<ConversationComposerState>),
) => void;

function attachmentScope(
  state: ConversationComposerState,
  ownerInput: string,
  conversationInput: string,
): AttachmentScope | null {
  const ownerUserId = ownerInput.trim();
  const conversationId = conversationInput.trim();
  if (!ownerUserId || !conversationId) return null;
  return {
    ownerUserId,
    conversationId,
    draftGeneration: state.attachmentDraftGeneration,
  };
}

function acceptsAttachmentScope(
  state: ConversationComposerState,
  scope: AttachmentScope,
): boolean {
  return state.ownerUserId === scope.ownerUserId
    && state.conversationId === scope.conversationId
    && state.attachmentDraftGeneration === scope.draftGeneration;
}

function beginAttachmentImport(set: ComposerSet) {
  set((state) => ({
    attachmentImportCount: state.attachmentImportCount + 1,
    isImportingAttachments: true,
    attachmentError: null,
  }));
}

function finishAttachmentImport(set: ComposerSet) {
  set((state) => {
    const attachmentImportCount = Math.max(0, state.attachmentImportCount - 1);
    return {
      attachmentImportCount,
      isImportingAttachments: attachmentImportCount > 0,
    };
  });
}

function applyAttachmentImport(
  get: () => ConversationComposerState,
  set: ComposerSet,
  port: ConversationAttachmentPort,
  result: AttachmentImportResult,
) {
  if (!acceptsAttachmentScope(get(), result.scope)) {
    discardImport(port, result);
    return;
  }
  set((state) => ({
    attachments: mergeAttachmentDrafts(state.attachments, result.attachments),
    attachmentError: result.errors.length ? result.errors.join("；") : null,
    isAttachmentDropTargeted: false,
  }));
}

function discardImport(port: ConversationAttachmentPort, result: AttachmentImportResult) {
  if (result.attachments.length === 0) return;
  void port.discard(result.scope, result.attachments.map(({ id }) => id));
}

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
