import { create } from "zustand";
import {
  configurationDraft,
  validateConfigurationDraft,
  type AiModelConfigurationDraft,
  type AiModelDefaults,
  type AiModelDraft,
  type AiModelProviderDraft,
  type AiModelSettingsSnapshot,
} from "../domain/aiModelSettings";
import type { AiModelSettingsPort } from "../ports/AiModelSettingsPort";

interface AiModelSettingsState {
  ownerUserId: string | null;
  snapshot: AiModelSettingsSnapshot | null;
  draft: AiModelConfigurationDraft | null;
  isLoading: boolean;
  isMutating: boolean;
  error: string | null;
  notice: string | null;
  epoch: number;
  load(port: AiModelSettingsPort, ownerUserId: string, force?: boolean): Promise<void>;
  updateDefaults(patch: Partial<AiModelDefaults>): void;
  updateModel(modelId: string, patch: Partial<Omit<AiModelDraft, "id">>): void;
  save(port: AiModelSettingsPort): Promise<boolean>;
  createProvider(port: AiModelSettingsPort, draft: AiModelProviderDraft): Promise<boolean>;
  updateProvider(port: AiModelSettingsPort, draft: AiModelProviderDraft): Promise<boolean>;
  refreshProvider(port: AiModelSettingsPort, providerId: string): Promise<boolean>;
  deleteProvider(port: AiModelSettingsPort, providerId: string): Promise<boolean>;
  clearMessage(): void;
  reset(): void;
}

export const useAiModelSettingsStore = create<AiModelSettingsState>((set, get) => ({
  ownerUserId: null,
  snapshot: null,
  draft: null,
  isLoading: false,
  isMutating: false,
  error: null,
  notice: null,
  epoch: 0,

  async load(port, ownerInput, force = false) {
    const ownerUserId = ownerInput.trim();
    if (!ownerUserId) {
      get().reset();
      return;
    }
    if (!force && get().ownerUserId === ownerUserId && (get().snapshot || get().isLoading)) return;
    const epoch = get().epoch + 1;
    set({ ownerUserId, snapshot: null, draft: null, isLoading: true, isMutating: false, error: null, notice: null, epoch });
    try {
      const snapshot = await port.load(ownerUserId);
      if (!isCurrent(get(), ownerUserId, epoch)) return;
      set({ snapshot, draft: configurationDraft(snapshot), isLoading: false });
    } catch (failure) {
      if (!isCurrent(get(), ownerUserId, epoch)) return;
      set({ isLoading: false, error: errorMessage(failure) });
    }
  },

  updateDefaults(patch) {
    set((state) => state.draft ? { draft: { ...state.draft, ...patch }, notice: null } : {});
  },

  updateModel(modelId, patch) {
    set((state) => {
      const current = state.draft?.modelDrafts[modelId];
      if (!state.draft || !current) return {};
      return {
        draft: {
          ...state.draft,
          modelDrafts: {
            ...state.draft.modelDrafts,
            [modelId]: { ...current, ...patch },
          },
        },
        notice: null,
      };
    });
  },

  async save(port) {
    const current = get();
    if (!current.ownerUserId || !current.snapshot || !current.draft || current.isMutating) return false;
    const validation = validateConfigurationDraft(current.draft, current.snapshot.models);
    if (validation) {
      set({ error: validation, notice: null });
      return false;
    }
    return runMutation(set, get, "AI 模型配置已保存。", (ownerUserId) => port.save(ownerUserId, current.draft!));
  },

  async createProvider(port, draft) {
    return runMutation(set, get, "供应商已添加，模型目录已同步。", (ownerUserId) => port.createProvider(ownerUserId, draft));
  },

  async updateProvider(port, draft) {
    return runMutation(set, get, "供应商配置已更新。", (ownerUserId) => port.updateProvider(ownerUserId, draft));
  },

  async refreshProvider(port, providerId) {
    return runMutation(set, get, "供应商模型目录已刷新。", (ownerUserId) => port.refreshProvider(ownerUserId, providerId));
  },

  async deleteProvider(port, providerId) {
    return runMutation(set, get, "供应商及其导入模型已删除。", (ownerUserId) => port.deleteProvider(ownerUserId, providerId));
  },

  clearMessage() { set({ error: null, notice: null }); },

  reset() {
    set((state) => ({
      ownerUserId: null,
      snapshot: null,
      draft: null,
      isLoading: false,
      isMutating: false,
      error: null,
      notice: null,
      epoch: state.epoch + 1,
    }));
  },
}));

export function isAiModelSettingsDirty(
  state: Pick<AiModelSettingsState, "snapshot" | "draft">,
) {
  if (!state.snapshot || !state.draft) return false;
  return JSON.stringify(configurationDraft(state.snapshot)) !== JSON.stringify(state.draft);
}

async function runMutation(
  set: (patch: Partial<AiModelSettingsState>) => void,
  get: () => AiModelSettingsState,
  notice: string,
  operation: (ownerUserId: string) => Promise<AiModelSettingsSnapshot>,
) {
  const current = get();
  if (!current.ownerUserId || current.isMutating) return false;
  const ownerUserId = current.ownerUserId;
  const epoch = current.epoch;
  set({ isMutating: true, error: null, notice: null });
  try {
    const snapshot = await operation(ownerUserId);
    if (!isCurrent(get(), ownerUserId, epoch)) return false;
    set({ snapshot, draft: configurationDraft(snapshot), isMutating: false, notice });
    return true;
  } catch (failure) {
    if (!isCurrent(get(), ownerUserId, epoch)) return false;
    set({ isMutating: false, error: errorMessage(failure) });
    return false;
  }
}

function isCurrent(state: AiModelSettingsState, ownerUserId: string, epoch: number) {
  return state.ownerUserId === ownerUserId && state.epoch === epoch;
}

function errorMessage(failure: unknown) {
  return failure instanceof Error ? failure.message : String(failure);
}
