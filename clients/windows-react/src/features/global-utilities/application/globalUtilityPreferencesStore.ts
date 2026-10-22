import { create } from "zustand";
import {
  defaultGlobalHotKeys,
  sameGlobalUtilityPreferences,
  type GlobalUtilityPreferences,
} from "../domain/globalUtilities";
import type { GlobalUtilityPreferencesPort } from "../ports/GlobalUtilityPreferencesPort";

const disabledRegistration = (requested: GlobalUtilityPreferences["hotKeys"][keyof GlobalUtilityPreferences["hotKeys"]]) => ({
  state: "disabled" as const,
  requested,
  active: null,
  usesFallback: false,
});

const defaults: GlobalUtilityPreferences = {
  isEnabled: false,
  hasAcknowledgedShortcutConflicts: false,
  screenshotEnabled: true,
  screenRecordingEnabled: true,
  clipboardHistoryEnabled: true,
  quickSearchEnabled: true,
  hotKeys: defaultGlobalHotKeys,
  registrations: {
    screenshot: disabledRegistration(defaultGlobalHotKeys.screenshot),
    screenRecording: disabledRegistration(defaultGlobalHotKeys.screenRecording),
    clipboardHistory: disabledRegistration(defaultGlobalHotKeys.clipboardHistory),
    quickSearch: disabledRegistration(defaultGlobalHotKeys.quickSearch),
  },
  revision: 1,
};

interface GlobalUtilityPreferencesState {
  ownerUserId: string | null;
  preferences: GlobalUtilityPreferences;
  savedPreferences: GlobalUtilityPreferences;
  isLoading: boolean;
  isSaving: boolean;
  error: string | null;
  epoch: number;
  draftRevision: number;
  activate(port: GlobalUtilityPreferencesPort, ownerUserId: string): Promise<void>;
  updateDraft(patch: Partial<Omit<GlobalUtilityPreferences, "revision" | "registrations">>): void;
  save(port: GlobalUtilityPreferencesPort): Promise<boolean>;
  clearError(): void;
  reset(): void;
}

export const useGlobalUtilityPreferencesStore = create<GlobalUtilityPreferencesState>((set, get) => ({
  ownerUserId: null,
  preferences: defaults,
  savedPreferences: defaults,
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
    set({ ownerUserId, preferences: defaults, savedPreferences: defaults, isLoading: true, isSaving: false, error: null, epoch, draftRevision: 0 });
    try {
      const preferences = await port.load(ownerUserId);
      const latest = get();
      if (latest.ownerUserId !== ownerUserId || latest.epoch !== epoch) return;
      set({ preferences, savedPreferences: preferences, isLoading: false, draftRevision: 1 });
    } catch (failure) {
      const latest = get();
      if (latest.ownerUserId !== ownerUserId || latest.epoch !== epoch) return;
      set({ isLoading: false, error: errorMessage(failure) });
    }
  },

  updateDraft(patch) {
    set((state) => ({
      preferences: { ...state.preferences, ...patch },
      draftRevision: state.draftRevision + 1,
    }));
  },

  async save(port) {
    const current = get();
    if (!current.ownerUserId || current.isLoading || current.isSaving) return false;
    if (sameGlobalUtilityPreferences(current.preferences, current.savedPreferences)) return true;
    const ownerUserId = current.ownerUserId;
    const epoch = current.epoch;
    const draftRevision = current.draftRevision;
    const submitted = current.preferences;
    set({ isSaving: true, error: null });
    try {
      const saved = await port.update(ownerUserId, submitted);
      const latest = get();
      if (latest.ownerUserId !== ownerUserId || latest.epoch !== epoch) return false;
      if (saved.revision <= submitted.revision) throw new Error("全局工具设置保存结果版本无效。");
      set({
        preferences: latest.draftRevision === draftRevision
          ? saved
          : { ...latest.preferences, registrations: saved.registrations, revision: saved.revision },
        savedPreferences: saved,
        isSaving: false,
      });
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
          set({ preferences: truth, savedPreferences: truth, draftRevision: after.draftRevision + 1, error: message });
        }
      } catch {
        // Keep the original native mutation error if reloading truth also fails.
      }
      return false;
    }
  },

  clearError() { set({ error: null }); },

  reset() {
    set((state) => ({
      ownerUserId: null,
      preferences: defaults,
      savedPreferences: defaults,
      isLoading: false,
      isSaving: false,
      error: null,
      epoch: state.epoch + 1,
      draftRevision: 0,
    }));
  },
}));

export function isGlobalUtilityPreferencesDirty(
  state: Pick<GlobalUtilityPreferencesState, "preferences" | "savedPreferences">,
) {
  return !sameGlobalUtilityPreferences(state.preferences, state.savedPreferences);
}

function errorMessage(failure: unknown) {
  return failure instanceof Error ? failure.message : String(failure);
}
