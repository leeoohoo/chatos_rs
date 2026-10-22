import { create } from "zustand";
import {
  sameGeneralSettings,
  type GeneralSettings,
} from "../domain/generalSettings";
import type { GeneralSettingsPort } from "../ports/GeneralSettingsPort";

const defaults: GeneralSettings = {
  interfaceLanguage: "zh-CN",
  contextLanguage: "zh-CN",
  interfaceFontSize: 14,
  preventsIdleSystemSleep: false,
  revision: 1,
};

interface GeneralSettingsState {
  ownerUserId: string | null;
  settings: GeneralSettings;
  savedSettings: GeneralSettings;
  isLoading: boolean;
  isSaving: boolean;
  error: string | null;
  epoch: number;
  draftRevision: number;
  activate(port: GeneralSettingsPort, ownerUserId: string): Promise<void>;
  updateDraft(patch: Partial<Omit<GeneralSettings, "revision">>): void;
  save(port: GeneralSettingsPort): Promise<boolean>;
  clearError(): void;
  reset(): void;
}

export const useGeneralSettingsStore = create<GeneralSettingsState>((set, get) => ({
  ownerUserId: null,
  settings: defaults,
  savedSettings: defaults,
  isLoading: false,
  isSaving: false,
  error: null,
  epoch: 0,
  draftRevision: 0,

  async activate(port, ownerInput) {
    const ownerUserId = ownerInput.trim();
    if (!ownerUserId) {
      get().reset();
      return;
    }
    if (get().ownerUserId === ownerUserId && !get().isLoading) return;
    const epoch = get().epoch + 1;
    set({ ownerUserId, settings: defaults, savedSettings: defaults, isLoading: true, isSaving: false, error: null, epoch, draftRevision: 0 });
    try {
      const settings = await port.load(ownerUserId);
      const latest = get();
      if (latest.ownerUserId !== ownerUserId || latest.epoch !== epoch) return;
      set({ settings, savedSettings: settings, isLoading: false, draftRevision: 1 });
      applyPresentation(settings);
    } catch (failure) {
      const latest = get();
      if (latest.ownerUserId !== ownerUserId || latest.epoch !== epoch) return;
      set({ isLoading: false, error: errorMessage(failure) });
    }
  },

  updateDraft(patch) {
    set((state) => {
      const settings = { ...state.settings, ...patch };
      applyPresentation(settings);
      return { settings, draftRevision: state.draftRevision + 1 };
    });
  },

  async save(port) {
    const current = get();
    if (!current.ownerUserId || current.isSaving || current.isLoading) return false;
    if (sameGeneralSettings(current.settings, current.savedSettings)) return true;
    const ownerUserId = current.ownerUserId;
    const epoch = current.epoch;
    const draftRevision = current.draftRevision;
    const submitted = current.settings;
    set({ isSaving: true, error: null });
    try {
      const saved = await port.update(ownerUserId, submitted);
      const latest = get();
      if (latest.ownerUserId !== ownerUserId || latest.epoch !== epoch) return false;
      if (saved.revision <= submitted.revision) throw new Error("常规设置保存结果版本无效。");
      set({
        settings: latest.draftRevision === draftRevision ? saved : { ...latest.settings, revision: saved.revision },
        savedSettings: saved,
        isSaving: false,
      });
      if (latest.draftRevision === draftRevision) applyPresentation(saved);
      return true;
    } catch (failure) {
      const latest = get();
      if (latest.ownerUserId !== ownerUserId || latest.epoch !== epoch) return false;
      const message = errorMessage(failure);
      set({ isSaving: false, error: message });
      try {
        const truth = await port.load(ownerUserId);
        const after = get();
        if (after.ownerUserId === ownerUserId && after.epoch === epoch) {
          set({ settings: truth, savedSettings: truth, draftRevision: after.draftRevision + 1, error: message });
          applyPresentation(truth);
        }
      } catch {
        // Preserve the original save error and local draft when Host truth is unavailable.
      }
      return false;
    }
  },

  clearError() { set({ error: null }); },

  reset() {
    set((state) => ({
      ownerUserId: null, settings: defaults, savedSettings: defaults, isLoading: false,
      isSaving: false, error: null, epoch: state.epoch + 1, draftRevision: 0,
    }));
  },
}));

export function isGeneralSettingsDirty(state: Pick<GeneralSettingsState, "settings" | "savedSettings">) {
  return !sameGeneralSettings(state.settings, state.savedSettings);
}

function applyPresentation(settings: GeneralSettings) {
  document.documentElement.lang = settings.interfaceLanguage;
  document.documentElement.style.setProperty("--interface-scale", String(settings.interfaceFontSize / 14));
}

function errorMessage(failure: unknown) {
  return failure instanceof Error ? failure.message : String(failure);
}
