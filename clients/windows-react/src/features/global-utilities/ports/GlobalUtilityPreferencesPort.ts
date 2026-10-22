import type { GlobalUtilityPreferences } from "../domain/globalUtilities";

export interface GlobalUtilityPreferencesPort {
  load(ownerUserId: string): Promise<GlobalUtilityPreferences>;
  update(ownerUserId: string, preferences: GlobalUtilityPreferences): Promise<GlobalUtilityPreferences>;
  deactivate(ownerUserId: string): Promise<void>;
}
