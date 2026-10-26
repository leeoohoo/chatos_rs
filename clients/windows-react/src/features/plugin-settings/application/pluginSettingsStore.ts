import { create } from "zustand";
import type { ManualPluginDirectorySelection, ManualPluginInspection, PluginSettingsSnapshot } from "../domain/pluginSettings";
import type { PluginSettingsPort } from "../ports/PluginSettingsPort";

interface PluginSettingsState {
  ownerUserId: string | null;
  snapshot: PluginSettingsSnapshot | null;
  isLoading: boolean;
  operatingPluginId: string | null;
  error: string | null;
  notice: string | null;
  manualSelection: ManualPluginDirectorySelection | null;
  manualInspection: ManualPluginInspection | null;
  manualBusy: "pick" | "inspect" | "install" | null;
  manualEpoch: number;
  epoch: number;
  load(port: PluginSettingsPort, ownerUserId: string): Promise<void>;
  install(port: PluginSettingsPort, pluginId: string): Promise<boolean>;
  updateEnabled(port: PluginSettingsPort, pluginId: string, enabled: boolean): Promise<boolean>;
  uninstall(port: PluginSettingsPort, pluginId: string): Promise<boolean>;
  updateCredentials(port: PluginSettingsPort, pluginId: string, credentials: Record<string, string | null>): Promise<boolean>;
  pickManualDirectory(port: PluginSettingsPort, presentationId: string): Promise<boolean>;
  inspectManualDirectory(port: PluginSettingsPort, presentationId: string): Promise<boolean>;
  installManualDirectory(port: PluginSettingsPort, presentationId: string): Promise<boolean>;
  discardManualDirectory(port: PluginSettingsPort, presentationId: string): Promise<void>;
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
  manualSelection: null,
  manualInspection: null,
  manualBusy: null,
  manualEpoch: 0,
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

  async updateCredentials(port, pluginId, credentials) {
    return runMutation(set, get, pluginId, "Plugin 凭据已安全保存。", (ownerUserId) => (
      port.updateCredentials(ownerUserId, pluginId, credentials)
    ));
  },

  async pickManualDirectory(port, presentationId) {
    const current = get();
    if (!current.ownerUserId || current.manualBusy || current.operatingPluginId) return false;
    const ownerUserId = current.ownerUserId;
    const epoch = current.epoch;
    const manualEpoch = current.manualEpoch + 1;
    set({ manualBusy: "pick", manualEpoch, error: null, notice: null });
    try {
      const selection = await port.pickManualDirectory(ownerUserId, presentationId);
      if (!isCurrent(get(), ownerUserId, epoch) || get().manualEpoch !== manualEpoch) return false;
      if (!selection) {
        set({ manualBusy: null });
        return false;
      }
      set({ manualBusy: null, manualSelection: selection, manualInspection: null });
      return true;
    } catch (failure) {
      if (!isCurrent(get(), ownerUserId, epoch) || get().manualEpoch !== manualEpoch) return false;
      set({ manualBusy: null, error: errorMessage(failure) });
      return false;
    }
  },

  async inspectManualDirectory(port, presentationId) {
    const current = get();
    if (!current.ownerUserId || !current.manualSelection || current.manualBusy || current.operatingPluginId) return false;
    const ownerUserId = current.ownerUserId;
    const selectionId = current.manualSelection.selectionId;
    const epoch = current.epoch;
    const manualEpoch = current.manualEpoch;
    set({ manualBusy: "inspect", manualInspection: null, error: null, notice: null });
    try {
      const inspection = await port.inspectManualDirectory(ownerUserId, presentationId, selectionId);
      if (!isCurrentManual(get(), ownerUserId, epoch, manualEpoch, selectionId)) return false;
      if (inspection.selectionId !== selectionId) {
        set({ manualBusy: null, error: "Plugin 预检响应与当前目录不匹配，请重新选择。" });
        return false;
      }
      set({ manualBusy: null, manualInspection: inspection });
      return true;
    } catch (failure) {
      if (!isCurrentManual(get(), ownerUserId, epoch, manualEpoch, selectionId)) return false;
      set({ manualBusy: null, error: errorMessage(failure) });
      return false;
    }
  },

  async installManualDirectory(port, presentationId) {
    const current = get();
    if (!current.ownerUserId || !current.manualSelection || !current.manualInspection || current.manualBusy || current.operatingPluginId) return false;
    const ownerUserId = current.ownerUserId;
    const selectionId = current.manualSelection.selectionId;
    const fingerprint = current.manualInspection.fingerprint;
    const epoch = current.epoch;
    const manualEpoch = current.manualEpoch;
    set({ manualBusy: "install", error: null, notice: null });
    try {
      const snapshot = await port.installManualDirectory(ownerUserId, presentationId, selectionId, fingerprint);
      if (!isCurrentManual(get(), ownerUserId, epoch, manualEpoch, selectionId)) return false;
      set({ snapshot, manualBusy: null, manualSelection: null, manualInspection: null, notice: "自定义 Plugin 已完成复检并安装到本机。" });
      return true;
    } catch (failure) {
      if (!isCurrentManual(get(), ownerUserId, epoch, manualEpoch, selectionId)) return false;
      set({ manualBusy: null, error: errorMessage(failure) });
      return false;
    }
  },

  async discardManualDirectory(port, presentationId) {
    const ownerUserId = get().ownerUserId;
    set((state) => ({
      manualSelection: null,
      manualInspection: null,
      manualBusy: null,
      manualEpoch: state.manualEpoch + 1,
    }));
    if (!ownerUserId) return;
    try {
      await port.discardManualDirectory(ownerUserId, presentationId);
    } catch {
      // Presentation teardown is best effort; native operation leases remain protected.
    }
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
      manualSelection: null,
      manualInspection: null,
      manualBusy: null,
      manualEpoch: state.manualEpoch + 1,
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

function isCurrentManual(
  state: PluginSettingsState,
  ownerUserId: string,
  epoch: number,
  manualEpoch: number,
  selectionId: string,
) {
  return isCurrent(state, ownerUserId, epoch)
    && state.manualEpoch === manualEpoch
    && state.manualSelection?.selectionId === selectionId;
}

function errorMessage(failure: unknown) {
  return failure instanceof Error ? failure.message : String(failure);
}
