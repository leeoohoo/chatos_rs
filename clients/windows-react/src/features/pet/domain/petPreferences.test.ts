import { describe, expect, it } from "vitest";
import { petPreferencesSchema } from "./petPreferences";

describe("pet preferences", () => {
  it("keeps the macOS size range and four-point step", () => {
    const base = { isEnabled: true, showProcess: true, showCompletions: true, showAcrossSpaces: true, revision: 1 };
    expect(petPreferencesSchema.safeParse({ ...base, size: 72 }).success).toBe(true);
    expect(petPreferencesSchema.safeParse({ ...base, size: 180 }).success).toBe(true);
    expect(petPreferencesSchema.safeParse({ ...base, size: 73 }).success).toBe(false);
    expect(petPreferencesSchema.safeParse({ ...base, size: 184 }).success).toBe(false);
  });
});
