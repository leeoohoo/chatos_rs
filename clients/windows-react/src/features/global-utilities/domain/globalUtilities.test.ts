import { describe, expect, it } from "vitest";
import {
  formatShortcut,
  globalUtilityPreferencesSchema,
} from "./globalUtilities";

describe("global utility preferences", () => {
  it("accepts only the complete native projection", () => {
    expect(globalUtilityPreferencesSchema.safeParse(preferences()).success).toBe(true);
    expect(globalUtilityPreferencesSchema.safeParse({ ...preferences(), localPath: "C:\\private" }).success).toBe(false);
    const missing = preferences();
    delete (missing.hotKeys as Partial<typeof missing.hotKeys>).quickSearch;
    expect(globalUtilityPreferencesSchema.safeParse(missing).success).toBe(false);
  });

  it("formats Windows modifier names without exposing native key codes", () => {
    expect(formatShortcut({ key: "K", modifiers: ["control", "shift"] })).toBe("Ctrl + Shift + K");
    expect(formatShortcut({ key: "Space", modifiers: ["super"] })).toBe("⊞ + Space");
  });
});

function preferences() {
  const hotKeys = {
    screenshot: { key: "A", modifiers: ["control" as const] },
    screenRecording: { key: "Q", modifiers: ["control" as const] },
    clipboardHistory: { key: "E", modifiers: ["control" as const] },
    quickSearch: { key: "Space", modifiers: ["control" as const] },
  };
  const registration = (action: keyof typeof hotKeys) => ({ state: "disabled" as const, requested: hotKeys[action], active: null, usesFallback: false });
  return {
    isEnabled: false,
    hasAcknowledgedShortcutConflicts: false,
    screenshotEnabled: true,
    screenRecordingEnabled: true,
    clipboardHistoryEnabled: true,
    quickSearchEnabled: true,
    hotKeys,
    registrations: {
      screenshot: registration("screenshot"),
      screenRecording: registration("screenRecording"),
      clipboardHistory: registration("clipboardHistory"),
      quickSearch: registration("quickSearch"),
    },
    revision: 1,
  };
}
