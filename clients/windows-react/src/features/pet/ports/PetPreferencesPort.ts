import type { PetPreferences } from "../domain/petPreferences";

export interface PetPreferencesPort {
  load(ownerUserId: string): Promise<PetPreferences>;
  update(ownerUserId: string, preferences: PetPreferences): Promise<PetPreferences>;
  resetPosition(ownerUserId: string, expectedRevision: number): Promise<PetPreferences>;
}
