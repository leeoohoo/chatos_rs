import { z } from "zod";

export const petPreferencesSchema = z.object({
  isEnabled: z.boolean(),
  size: z.number().int().min(72).max(180).refine((value) => (value - 72) % 4 === 0),
  showProcess: z.boolean(),
  showCompletions: z.boolean(),
  showAcrossSpaces: z.boolean(),
  revision: z.number().int().positive(),
}).strict();

export type PetPreferences = z.infer<typeof petPreferencesSchema>;

export function samePetPreferences(left: PetPreferences, right: PetPreferences) {
  return left.isEnabled === right.isEnabled
    && left.size === right.size
    && left.showProcess === right.showProcess
    && left.showCompletions === right.showCompletions
    && left.showAcrossSpaces === right.showAcrossSpaces;
}
