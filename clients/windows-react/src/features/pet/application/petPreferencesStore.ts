import { create } from "zustand";
import {
  samePetPreferences,
  type PetPreferences,
} from "../domain/petPreferences";
import type { PetPreferencesPort } from "../ports/PetPreferencesPort";

const defaults: PetPreferences = {
  isEnabled: true,
  size: 104,
  showProcess: true,
  showCompletions: true,
  showAcrossSpaces: true,
  revision: 1,
};

interface PetPreferencesState {
  ownerUserId: string | null;
  preferences: PetPreferences;
  savedPreferences: PetPreferences;
  isLoading: boolean;
  isSaving: boolean;
  error: string | null;
  epoch: number;
  draftRevision: number;
  activate(port: PetPreferencesPort, ownerUserId: string): Promise<void>;
  updateDraft(patch: Partial<Omit<PetPreferences, "revision">>): void;
  save(port: PetPreferencesPort): Promise<boolean>;
  resetPosition(port: PetPreferencesPort): Promise<boolean>;
  clearError(): void;
  reset(): void;
}

export const usePetPreferencesStore = create<PetPreferencesState>((set, get) => ({
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
    if (samePetPreferences(current.preferences, current.savedPreferences)) return true;
    const ownerUserId = current.ownerUserId;
    const epoch = current.epoch;
    const draftRevision = current.draftRevision;
    const submitted = current.preferences;
    set({ isSaving: true, error: null });
    try {
      const saved = await port.update(ownerUserId, submitted);
      const latest = get();
      if (latest.ownerUserId !== ownerUserId || latest.epoch !== epoch) return false;
      if (saved.revision <= submitted.revision) throw new Error("宠物设置保存结果版本无效。");
      set({
        preferences: latest.draftRevision === draftRevision
          ? saved
          : { ...latest.preferences, revision: saved.revision },
        savedPreferences: saved,
        isSaving: false,
      });
      return true;
    } catch (failure) {
      return recover(port, ownerUserId, epoch, failure, set, get);
    }
  },

  async resetPosition(port) {
    const current = get();
    if (!current.ownerUserId || current.isLoading || current.isSaving
      || !samePetPreferences(current.preferences, current.savedPreferences)) return false;
    const ownerUserId = current.ownerUserId;
    const epoch = current.epoch;
    set({ isSaving: true, error: null });
    try {
      const saved = await port.resetPosition(ownerUserId, current.savedPreferences.revision);
      const latest = get();
      if (latest.ownerUserId !== ownerUserId || latest.epoch !== epoch) return false;
      set({ preferences: saved, savedPreferences: saved, isSaving: false, draftRevision: latest.draftRevision + 1 });
      return true;
    } catch (failure) {
      return recover(port, ownerUserId, epoch, failure, set, get);
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

export function isPetPreferencesDirty(state: Pick<PetPreferencesState, "preferences" | "savedPreferences">) {
  return !samePetPreferences(state.preferences, state.savedPreferences);
}

async function recover(
  port: PetPreferencesPort,
  ownerUserId: string,
  epoch: number,
  failure: unknown,
  set: (partial: Partial<PetPreferencesState>) => void,
  get: () => PetPreferencesState,
) {
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
    // Preserve the original mutation error when native truth is unavailable.
  }
  return false;
}

function errorMessage(failure: unknown) {
  return failure instanceof Error ? failure.message : String(failure);
}
