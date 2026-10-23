import { create } from "zustand";
import type { PluginSettingsSnapshot } from "../domain/pluginSettings";
import type { PluginSettingsPort } from "../ports/PluginSettingsPort";

interface PluginSettingsState {
  ownerUserId: string | null;
  snapshot: PluginSettingsSnapshot | null;
  isLoading: boolean;
  operatingPluginId: string | null;
  error: string | null;
  notice: string | null;
  epoch: number;
  load(port: PluginSettingsPort, ownerUserId: string): Promise<void>;
  install(port: PluginSettingsPort, pluginId: string): Promise<boolean>;
  updateEnabled(port: PluginSettingsPort, pluginId: string, enabled: boolean): Promise<boolean>;
  uninstall(port: PluginSettingsPort, pluginId: string): Promise<boolean>;
  clearMessage(): void;
  reset(): void;
}

export const usePluginSettingsStore = create<PluginSettingsState>((set, get) => ({
  ownerUserId: null,
  snapshot: null,
  isLoading: false,
  operatingPluginId: null,
  error: null,
  notice: null,
  epoch: 0,

  async load(port, ownerInput) {
    const ownerUserId = ownerInput.trim();
    if (!ownerUserId) {
      get().reset();
      return;
    }
    const epoch = get().epoch + 1;
    set({ ownerUserId, isLoading: true, error: null, notice: null, epoch });
    try {
      const snapshot = await port.load(ownerUserId);
      if (!isCurrent(get(), ownerUserId, epoch)) return;
      set({ snapshot, isLoading: false });
    } catch (failure) {
      if (!isCurrent(get(), ownerUserId, epoch)) return;
      set({ isLoading: false, error: errorMessage(failure) });
    }
  },

  async updateEnabled(port, pluginId, enabled) {
    return runMutation(set, get, pluginId, enabled ? "Plugin 已启用。" : "Plugin 已停用。", (ownerUserId) => (
      port.updateEnabled(ownerUserId, pluginId, enabled)
    ));
  },

  async install(port, pluginId) {
    const installed = get().snapshot?.plugins.find((plugin) => plugin.id === pluginId)?.installed;
    return runMutation(set, get, pluginId, installed ? "Plugin 已安全更新。" : "Plugin 已安全安装。", (ownerUserId) => (
      port.install(ownerUserId, pluginId)
    ));
  },

  async uninstall(port, pluginId) {
    return runMutation(set, get, pluginId, "Plugin 已从本机卸载。", (ownerUserId) => (
      port.uninstall(ownerUserId, pluginId)
    ));
  },

  clearMessage() { set({ error: null, notice: null }); },

  reset() {
    set((state) => ({
      ownerUserId: null,
      snapshot: null,
      isLoading: false,
      operatingPluginId: null,
      error: null,
      notice: null,
      epoch: state.epoch + 1,
    }));
  },
}));

async function runMutation(
  set: (patch: Partial<PluginSettingsState>) => void,
  get: () => PluginSettingsState,
  pluginId: string,
  notice: string,
  operation: (ownerUserId: string) => Promise<PluginSettingsSnapshot>,
) {
  const current = get();
  if (!current.ownerUserId || current.operatingPluginId) return false;
  const ownerUserId = current.ownerUserId;
  const epoch = current.epoch;
  set({ operatingPluginId: pluginId, error: null, notice: null });
  try {
    const snapshot = await operation(ownerUserId);
    if (!isCurrent(get(), ownerUserId, epoch)) return false;
    set({ snapshot, operatingPluginId: null, notice });
    return true;
  } catch (failure) {
    if (!isCurrent(get(), ownerUserId, epoch)) return false;
    set({ operatingPluginId: null, error: errorMessage(failure) });
    return false;
  }
}

function isCurrent(state: PluginSettingsState, ownerUserId: string, epoch: number) {
  return state.ownerUserId === ownerUserId && state.epoch === epoch;
}

function errorMessage(failure: unknown) {
  return failure instanceof Error ? failure.message : String(failure);
}
