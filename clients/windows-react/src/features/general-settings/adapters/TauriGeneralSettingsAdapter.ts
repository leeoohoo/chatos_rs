import { invoke } from "@tauri-apps/api/core";
import { generalSettingsSchema, type GeneralSettings } from "../domain/generalSettings";
import type { GeneralSettingsPort } from "../ports/GeneralSettingsPort";

export class TauriGeneralSettingsAdapter implements GeneralSettingsPort {
  async load(ownerUserId: string) {
    return generalSettingsSchema.parse(await invoke("load_general_settings", { ownerUserId }));
  }

  async update(ownerUserId: string, settings: GeneralSettings) {
    return generalSettingsSchema.parse(await invoke("update_general_settings", {
      input: {
        ownerUserId,
        interfaceLanguage: settings.interfaceLanguage,
        contextLanguage: settings.contextLanguage,
        interfaceFontSize: settings.interfaceFontSize,
        preventsIdleSystemSleep: settings.preventsIdleSystemSleep,
        expectedRevision: settings.revision,
      },
    }));
  }
}
