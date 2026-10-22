import { invoke } from "@tauri-apps/api/core";
import { petPreferencesSchema, type PetPreferences } from "../domain/petPreferences";
import type { PetPreferencesPort } from "../ports/PetPreferencesPort";

export class TauriPetPreferencesAdapter implements PetPreferencesPort {
  async load(ownerUserId: string) {
    return petPreferencesSchema.parse(await invoke("load_pet_preferences", { ownerUserId }));
  }

  async update(ownerUserId: string, preferences: PetPreferences) {
    return petPreferencesSchema.parse(await invoke("update_pet_preferences", {
      input: {
        ownerUserId,
        isEnabled: preferences.isEnabled,
        size: preferences.size,
        showProcess: preferences.showProcess,
        showCompletions: preferences.showCompletions,
        showAcrossSpaces: preferences.showAcrossSpaces,
        expectedRevision: preferences.revision,
      },
    }));
  }

  async resetPosition(ownerUserId: string, expectedRevision: number) {
    return petPreferencesSchema.parse(await invoke("reset_pet_position", {
      ownerUserId,
      expectedRevision,
    }));
  }
}
