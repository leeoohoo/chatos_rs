import { create } from "zustand";
import type { ConversationRuntimeSettingsSnapshot } from "../domain/conversationRuntimeSettings";
import type {
  ConversationRuntimeSettingsPort,
  RuntimeSettingsScope,
} from "../ports/ConversationRuntimeSettingsPort";

interface RuntimeSettingsState {
  ownerUserId: string | null;
  conversationId: string | null;
  snapshot: ConversationRuntimeSettingsSnapshot | null;
  isLoading: boolean;
  isUpdating: boolean;
  error: string | null;
  sequence: number;
  requestId: number;
  load(port: ConversationRuntimeSettingsPort, ownerUserId: string, conversationId: string): Promise<void>;
  selectModel(port: ConversationRuntimeSettingsPort, modelId: string): Promise<void>;
  selectReasoningLevel(port: ConversationRuntimeSettingsPort, level: string): Promise<void>;
  selectRemoteConnection(
    port: ConversationRuntimeSettingsPort,
    connectionId: string | null,
  ): Promise<void>;
  reset(): void;
}

type Mutation =
  | { kind: "model"; value: string }
  | { kind: "reasoning"; value: string }
  | { kind: "remote"; value: string | null };

export const useConversationRuntimeSettingsStore = create<RuntimeSettingsState>((set, get) => ({
  ownerUserId: null,
  conversationId: null,
  snapshot: null,
  isLoading: false,
  isUpdating: false,
  error: null,
  sequence: 0,
  requestId: 0,

  async load(port, ownerInput, conversationInput) {
    const scope = normalizedScope(ownerInput, conversationInput);
    if (!scope) return;
    const requestId = get().sequence + 1;
    set({
      ...scope,
      snapshot: null,
      isLoading: true,
      isUpdating: false,
      error: null,
      sequence: requestId,
      requestId,
    });
    try {
      const snapshot = await port.load(scope);
      if (!accepts(get, scope, requestId)) return;
      set({ snapshot, isLoading: false, requestId: 0 });
    } catch (error) {
      if (!accepts(get, scope, requestId)) return;
      set({ isLoading: false, error: errorMessage(error), requestId: 0 });
    }
  },

  async selectModel(port, modelId) {
    await mutate(port, get, set, { kind: "model", value: modelId });
  },

  async selectReasoningLevel(port, level) {
    await mutate(port, get, set, { kind: "reasoning", value: level });
  },

  async selectRemoteConnection(port, connectionId) {
    await mutate(port, get, set, { kind: "remote", value: connectionId });
  },

  reset() {
    set((state) => ({
      ownerUserId: null,
      conversationId: null,
      snapshot: null,
      isLoading: false,
      isUpdating: false,
      error: null,
      sequence: state.sequence + 1,
      requestId: 0,
    }));
  },
}));

async function mutate(
  port: ConversationRuntimeSettingsPort,
  get: () => RuntimeSettingsState,
  set: (partial: Partial<RuntimeSettingsState>) => void,
  mutation: Mutation,
) {
  const current = get();
  if (!current.snapshot || !current.ownerUserId || !current.conversationId) return;
  const scope = { ownerUserId: current.ownerUserId, conversationId: current.conversationId };
  const requestId = current.sequence + 1;
  const previous = current.snapshot;
  set({
    snapshot: optimisticSnapshot(previous, mutation),
    isLoading: false,
    isUpdating: true,
    error: null,
    sequence: requestId,
    requestId,
  });
  try {
    const snapshot = mutation.kind === "model"
      ? await port.selectModel({ ...scope, modelId: mutation.value })
      : mutation.kind === "reasoning"
        ? await port.selectReasoningLevel({ ...scope, level: mutation.value })
        : await port.selectRemoteConnection({ ...scope, connectionId: mutation.value });
    if (!accepts(get, scope, requestId)) return;
    set({ snapshot, isUpdating: false, requestId: 0 });
  } catch (error) {
    if (!accepts(get, scope, requestId)) return;
    set({
      snapshot: previous,
      isUpdating: false,
      error: errorMessage(error),
      requestId: 0,
    });
  }
}

function optimisticSnapshot(
  snapshot: ConversationRuntimeSettingsSnapshot,
  mutation: Mutation,
): ConversationRuntimeSettingsSnapshot {
  if (mutation.kind === "model") {
    const model = snapshot.models.find((candidate) => candidate.id === mutation.value);
    if (!model) return snapshot;
    return {
      ...snapshot,
      settings: {
        ...snapshot.settings,
        selectedModelId: model.id,
        selectedThinkingLevel: model.defaultThinkingLevel,
        reasoningEnabled: model.defaultThinkingLevel !== "none",
      },
    };
  }
  if (mutation.kind === "reasoning") {
    return {
      ...snapshot,
      settings: {
        ...snapshot.settings,
        selectedThinkingLevel: mutation.value,
        reasoningEnabled: mutation.value !== "none",
      },
    };
  }
  return {
    ...snapshot,
    settings: { ...snapshot.settings, remoteConnectionId: mutation.value },
  };
}

function normalizedScope(ownerUserId: string, conversationId: string): RuntimeSettingsScope | null {
  const owner = ownerUserId.trim();
  const conversation = conversationId.trim();
  return owner && conversation ? { ownerUserId: owner, conversationId: conversation } : null;
}

function accepts(
  get: () => RuntimeSettingsState,
  scope: RuntimeSettingsScope,
  requestId: number,
) {
  const state = get();
  return state.ownerUserId === scope.ownerUserId
    && state.conversationId === scope.conversationId
    && state.requestId === requestId;
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "无法更新会话运行配置。";
}
