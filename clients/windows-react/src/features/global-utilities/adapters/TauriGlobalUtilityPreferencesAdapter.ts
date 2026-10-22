import { invoke } from "@tauri-apps/api/core";
import {
  globalUtilityPreferencesSchema,
  type GlobalUtilityPreferences,
} from "../domain/globalUtilities";
import type { GlobalUtilityPreferencesPort } from "../ports/GlobalUtilityPreferencesPort";

export class TauriGlobalUtilityPreferencesAdapter implements GlobalUtilityPreferencesPort {
  async load(ownerUserId: string) {
    return globalUtilityPreferencesSchema.parse(await invoke("load_global_utility_preferences", { ownerUserId }));
  }

  async update(ownerUserId: string, preferences: GlobalUtilityPreferences) {
    return globalUtilityPreferencesSchema.parse(await invoke("update_global_utility_preferences", {
      input: {
        ownerUserId,
        isEnabled: preferences.isEnabled,
        hasAcknowledgedShortcutConflicts: preferences.hasAcknowledgedShortcutConflicts,
        screenshotEnabled: preferences.screenshotEnabled,
        screenRecordingEnabled: preferences.screenRecordingEnabled,
        clipboardHistoryEnabled: preferences.clipboardHistoryEnabled,
        quickSearchEnabled: preferences.quickSearchEnabled,
        hotKeys: preferences.hotKeys,
        expectedRevision: preferences.revision,
      },
    }));
  }

  async deactivate(ownerUserId: string) {
    await invoke("deactivate_global_utilities", { ownerUserId });
  }
}
