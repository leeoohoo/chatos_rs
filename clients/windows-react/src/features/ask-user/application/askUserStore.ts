import { create } from "zustand";
import { upsertAskUserPrompt, type AskUserPrompt, type AskUserSubmission } from "../domain/askUser";
import type { AskUserPort } from "../ports/AskUserPort";

interface AskUserState {
  ownerUserId: string | null;
  conversationId: string | null;
  prompts: AskUserPrompt[];
  isLoading: boolean;
  hasLoaded: boolean;
  loadError: string | null;
  requestSequence: number;
  activeRefreshId: number;
  actionIds: Record<string, number>;
  promptErrors: Record<string, string>;
  activate(port: AskUserPort, ownerUserId: string, conversationId: string): Promise<void>;
  refresh(port: AskUserPort): Promise<void>;
  submit(port: AskUserPort, prompt: AskUserPrompt, submission: AskUserSubmission): Promise<boolean>;
  cancel(port: AskUserPort, prompt: AskUserPrompt): Promise<boolean>;
  clearPromptError(promptId: string): void;
  reset(ownerUserId?: string): void;
}
export const useAskUserStore = create<AskUserState>((set, get) => ({
  ownerUserId: null,
  conversationId: null,
  prompts: [],
  isLoading: false,
  hasLoaded: false,
  loadError: null,
  requestSequence: 0,
  activeRefreshId: 0,
  actionIds: {},
  promptErrors: {},

  async activate(port, ownerInput, conversationInput) {
    const ownerUserId = ownerInput.trim();
    const conversationId = conversationInput.trim();
    if (!ownerUserId || !conversationId) return;
    const current = get();
    if (current.ownerUserId !== ownerUserId || current.conversationId !== conversationId) {
      set({
        ownerUserId,
        conversationId,
        prompts: [],
        isLoading: false,
        hasLoaded: false,
        loadError: null,
        requestSequence: current.requestSequence + 1,
        activeRefreshId: 0,
        actionIds: {},
        promptErrors: {},
      });
    }
    if (get().hasLoaded || get().isLoading) return;
    await get().refresh(port);
  },

  async refresh(port) {
    const current = get();
    if (!current.ownerUserId || !current.conversationId) return;
    const requestId = current.requestSequence + 1;
    const { ownerUserId, conversationId } = current;
    set({
      requestSequence: requestId,
      activeRefreshId: requestId,
      isLoading: !current.hasLoaded,
      loadError: null,
    });
    try {
      const prompts = await port.loadPrompts(ownerUserId, conversationId);
      if (!acceptsRefresh(get(), ownerUserId, conversationId, requestId)) return;
      set({ prompts, isLoading: false, hasLoaded: true, loadError: null, activeRefreshId: 0 });
    } catch (error) {
      if (!acceptsRefresh(get(), ownerUserId, conversationId, requestId)) return;
      set({ isLoading: false, hasLoaded: true, loadError: errorMessage(error), activeRefreshId: 0 });
    }
  },

  async submit(port, prompt, submission) {
    return mutate(set, get, port, prompt, () => port.submitPrompt({
      ownerUserId: get().ownerUserId ?? "",
      conversationId: get().conversationId ?? "",
      promptId: prompt.id,
      submission,
    }));
  },

  async cancel(port, prompt) {
    if (!prompt.allowsCancel) return false;
    return mutate(set, get, port, prompt, () => port.cancelPrompt({
      ownerUserId: get().ownerUserId ?? "",
      conversationId: get().conversationId ?? "",
      promptId: prompt.id,
    }));
  },

  clearPromptError(promptId) {
    set((state) => {
      const promptErrors = { ...state.promptErrors };
      delete promptErrors[promptId];
      return { promptErrors };
    });
  },

  reset(ownerUserId = "") {
    set((state) => ({
      ownerUserId: ownerUserId.trim() || null,
      conversationId: null,
      prompts: [],
      isLoading: false,
      hasLoaded: false,
      loadError: null,
      requestSequence: state.requestSequence + 1,
      activeRefreshId: 0,
      actionIds: {},
      promptErrors: {},
    }));
  },
}));

type SetState = (value: Partial<AskUserState> | ((state: AskUserState) => Partial<AskUserState>)) => void;
type GetState = () => AskUserState;

async function mutate(
  set: SetState,
  get: GetState,
  port: AskUserPort,
  prompt: AskUserPrompt,
  operation: () => Promise<AskUserPrompt>,
) {
  const current = get();
  if (!current.ownerUserId || !current.conversationId
    || prompt.sessionId !== current.conversationId
    || prompt.status !== "pending"
    || current.actionIds[prompt.id] != null) return false;
  const requestId = current.requestSequence + 1;
  const ownerUserId = current.ownerUserId;
  const conversationId = current.conversationId;
  set((state) => {
    const promptErrors = { ...state.promptErrors };
    delete promptErrors[prompt.id];
    return {
      requestSequence: requestId,
      activeRefreshId: 0,
      actionIds: { ...state.actionIds, [prompt.id]: requestId },
      promptErrors,
    };
  });
  try {
    const updated = await operation();
    if (!acceptsAction(get(), ownerUserId, conversationId, prompt.id, requestId)) return false;
    finishAction(set, prompt.id, {
      prompts: upsertAskUserPrompt(get().prompts, updated),
      activeRefreshId: 0,
    });
    return true;
  } catch (error) {
    if (!acceptsAction(get(), ownerUserId, conversationId, prompt.id, requestId)) return false;
    const message = errorMessage(error);
    set((state) => ({
      promptErrors: { ...state.promptErrors, [prompt.id]: message },
      activeRefreshId: 0,
    }));
    try {
      const prompts = await port.loadPrompts(ownerUserId, conversationId);
      if (!acceptsAction(get(), ownerUserId, conversationId, prompt.id, requestId)) return false;
      finishAction(set, prompt.id, { prompts });
    } catch {
      if (!acceptsAction(get(), ownerUserId, conversationId, prompt.id, requestId)) return false;
      finishAction(set, prompt.id);
    }
    return false;
  }
}

function finishAction(set: SetState, promptId: string, extra: Partial<AskUserState> = {}) {
  set((state) => {
    const actionIds = { ...state.actionIds };
    delete actionIds[promptId];
    return { ...extra, actionIds };
  });
}

function acceptsRefresh(
  state: AskUserState,
  ownerUserId: string,
  conversationId: string,
  requestId: number,
) {
  return state.ownerUserId === ownerUserId
    && state.conversationId === conversationId
    && state.activeRefreshId === requestId;
}

function acceptsAction(
  state: AskUserState,
  ownerUserId: string,
  conversationId: string,
  promptId: string,
  requestId: number,
) {
  return state.ownerUserId === ownerUserId
    && state.conversationId === conversationId
    && state.actionIds[promptId] === requestId;
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "交互请求处理失败。";
}
