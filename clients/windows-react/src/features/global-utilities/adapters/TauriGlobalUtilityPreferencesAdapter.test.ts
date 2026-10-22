import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { GlobalUtilityPreferences } from "../domain/globalUtilities";
import { TauriGlobalUtilityPreferencesAdapter } from "./TauriGlobalUtilityPreferencesAdapter";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("TauriGlobalUtilityPreferencesAdapter", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("uses revisioned native commands and a separate teardown command", async () => {
    const adapter = new TauriGlobalUtilityPreferencesAdapter();
    vi.mocked(invoke).mockResolvedValueOnce(preferences());
    await adapter.load("owner-1");
    expect(invoke).toHaveBeenLastCalledWith("load_global_utility_preferences", { ownerUserId: "owner-1" });

    vi.mocked(invoke).mockResolvedValueOnce(preferences({ isEnabled: true, revision: 2 }));
    await adapter.update("owner-1", preferences({ isEnabled: true }));
    expect(invoke).toHaveBeenLastCalledWith("update_global_utility_preferences", {
      input: expect.objectContaining({ ownerUserId: "owner-1", isEnabled: true, expectedRevision: 1 }),
    });

    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    await adapter.deactivate("owner-1");
    expect(invoke).toHaveBeenLastCalledWith("deactivate_global_utilities", { ownerUserId: "owner-1" });
  });
});

function preferences(overrides: Partial<GlobalUtilityPreferences> = {}): GlobalUtilityPreferences {
  const hotKeys = {
    screenshot: { key: "A", modifiers: ["control" as const] },
    screenRecording: { key: "Q", modifiers: ["control" as const] },
    clipboardHistory: { key: "E", modifiers: ["control" as const] },
    quickSearch: { key: "Space", modifiers: ["control" as const] },
  };
  const registration = (action: keyof typeof hotKeys) => ({ state: "disabled" as const, requested: hotKeys[action], active: null, usesFallback: false });
  return {
    isEnabled: false, hasAcknowledgedShortcutConflicts: false,
    screenshotEnabled: true, screenRecordingEnabled: true, clipboardHistoryEnabled: true, quickSearchEnabled: true,
    hotKeys,
    registrations: {
      screenshot: registration("screenshot"), screenRecording: registration("screenRecording"),
      clipboardHistory: registration("clipboardHistory"), quickSearch: registration("quickSearch"),
    },
    revision: 1,
    ...overrides,
  };
}
