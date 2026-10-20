import { create } from "zustand";
import type {
  ToolApprovalMode,
  ToolApprovalSettings,
} from "../domain/toolApprovalSettings";
import type { ToolApprovalSettingsPort } from "../ports/ToolApprovalSettingsPort";

interface ToolApprovalSettingsState {
  ownerUserId: string | null;
  hostAvailable: boolean;
  settings: ToolApprovalSettings | null;
  isLoading: boolean;
  isMutating: boolean;
  loadError: string | null;
  mutationError: string | null;
  requestSequence: number;
  activeLoadId: number;
  activeMutationId: number;
  activate(port: ToolApprovalSettingsPort, ownerUserId: string): Promise<void>;
  suspend(ownerUserId: string): void;
  load(port: ToolApprovalSettingsPort): Promise<void>;
  updateMode(
    port: ToolApprovalSettingsPort,
    mode: ToolApprovalMode,
    riskAcknowledged: boolean,
  ): Promise<boolean>;
  updateModel(
    port: ToolApprovalSettingsPort,
    selectedModelId: string | null,
    selectedThinkingLevel: string | null,
  ): Promise<boolean>;
  clearMutationError(): void;
  reset(): void;
}

export const useToolApprovalSettingsStore = create<ToolApprovalSettingsState>((set, get) => ({
  ownerUserId: null,
  hostAvailable: false,
  settings: null,
  isLoading: false,
  isMutating: false,
  loadError: null,
  mutationError: null,
  requestSequence: 0,
  activeLoadId: 0,
  activeMutationId: 0,

  async activate(port, ownerInput) {
    const ownerUserId = ownerInput.trim();
    if (!ownerUserId) return;
    const current = get();
    if (current.ownerUserId !== ownerUserId) {
      set({
        ownerUserId,
        hostAvailable: true,
        settings: null,
        isLoading: false,
        isMutating: false,
        loadError: null,
        mutationError: null,
        requestSequence: current.requestSequence + 1,
        activeLoadId: 0,
        activeMutationId: 0,
      });
    } else if (!current.hostAvailable) {
      set({
        hostAvailable: true,
        requestSequence: current.requestSequence + 1,
        activeLoadId: 0,
        activeMutationId: 0,
      });
    }
    await get().load(port);
  },

  suspend(ownerInput) {
    const ownerUserId = ownerInput.trim();
    const current = get();
    if (!ownerUserId || current.ownerUserId !== ownerUserId || !current.hostAvailable) return;
    set({
      hostAvailable: false,
      isLoading: false,
      isMutating: false,
      requestSequence: current.requestSequence + 1,
      activeLoadId: 0,
      activeMutationId: 0,
    });
  },

  async load(port) {
    const current = get();
    if (!current.ownerUserId || !current.hostAvailable || current.isMutating) return;
    const ownerUserId = current.ownerUserId;
    const requestId = current.requestSequence + 1;
    set({
      requestSequence: requestId,
      activeLoadId: requestId,
      isLoading: current.settings == null,
      loadError: null,
    });
    try {
      const settings = await port.loadSettings(ownerUserId);
      if (!acceptsLoad(get(), ownerUserId, requestId)) return;
      set({ settings, isLoading: false, loadError: null, activeLoadId: 0 });
    } catch (error) {
      if (!acceptsLoad(get(), ownerUserId, requestId)) return;
      set({
        isLoading: false,
        loadError: errorMessage(error, "审批设置暂时不可用。"),
        activeLoadId: 0,
      });
    }
  },

  async updateMode(port, mode, riskAcknowledged) {
    const current = get();
    if (!current.ownerUserId || !current.hostAvailable || current.isMutating) return false;
    const ownerUserId = current.ownerUserId;
    return mutate(set, get, ownerUserId, async () => port.updateMode({
      ownerUserId,
      mode,
      riskAcknowledged,
    }));
  },

  async updateModel(port, selectedModelId, selectedThinkingLevel) {
    const current = get();
    if (!current.ownerUserId || !current.hostAvailable || current.isMutating) return false;
    const ownerUserId = current.ownerUserId;
    return mutate(set, get, ownerUserId, async () => port.updateModel({
      ownerUserId,
      selectedModelId,
      selectedThinkingLevel,
    }));
  },

  clearMutationError() {
    set({ mutationError: null });
  },

  reset() {
    set((state) => ({
      ownerUserId: null,
      hostAvailable: false,
      settings: null,
      isLoading: false,
      isMutating: false,
      loadError: null,
      mutationError: null,
      requestSequence: state.requestSequence + 1,
      activeLoadId: 0,
      activeMutationId: 0,
    }));
  },
}));

type SetState = (
  value: Partial<ToolApprovalSettingsState>
    | ((state: ToolApprovalSettingsState) => Partial<ToolApprovalSettingsState>),
) => void;
type GetState = () => ToolApprovalSettingsState;

async function mutate(
  set: SetState,
  get: GetState,
  ownerUserId: string,
  operation: () => Promise<ToolApprovalSettings>,
) {
  const mutationId = get().requestSequence + 1;
  set({
    requestSequence: mutationId,
    activeLoadId: 0,
    activeMutationId: mutationId,
    isLoading: false,
    isMutating: true,
    mutationError: null,
  });
  try {
    const settings = await operation();
    if (!acceptsMutation(get(), ownerUserId, mutationId)) return false;
    set({
      settings,
      isMutating: false,
      mutationError: null,
      activeMutationId: 0,
    });
    return true;
  } catch (error) {
    if (!acceptsMutation(get(), ownerUserId, mutationId)) return false;
    set({
      isMutating: false,
      mutationError: errorMessage(error, "审批设置保存失败。"),
      activeMutationId: 0,
    });
    return false;
  }
}

function acceptsLoad(
  state: ToolApprovalSettingsState,
  ownerUserId: string,
  requestId: number,
) {
  return state.ownerUserId === ownerUserId
    && state.hostAvailable
    && state.activeLoadId === requestId;
}

function acceptsMutation(
  state: ToolApprovalSettingsState,
  ownerUserId: string,
  mutationId: number,
) {
  return state.ownerUserId === ownerUserId
    && state.hostAvailable
    && state.activeMutationId === mutationId;
}

function errorMessage(error: unknown, fallback: string) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return fallback;
}
