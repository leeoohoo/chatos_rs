import type { GeneralSettings } from "../domain/generalSettings";

export interface GeneralSettingsPort {
  load(ownerUserId: string): Promise<GeneralSettings>;
  update(ownerUserId: string, settings: GeneralSettings): Promise<GeneralSettings>;
}
