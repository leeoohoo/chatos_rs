import { beforeEach, describe, expect, it, vi } from "vitest";
import type { GlobalUtilityPreferences } from "../domain/globalUtilities";
import type { GlobalUtilityPreferencesPort } from "../ports/GlobalUtilityPreferencesPort";
import { useGlobalUtilityPreferencesStore } from "./globalUtilityPreferencesStore";

describe("global utility preferences store", () => {
  beforeEach(() => useGlobalUtilityPreferencesStore.getState().reset());

  it("version-saves device preferences", async () => {
    const port = stubPort();
    await useGlobalUtilityPreferencesStore.getState().activate(port, "owner-1");
    useGlobalUtilityPreferencesStore.getState().updateDraft({ isEnabled: true, hasAcknowledgedShortcutConflicts: true });
    await expect(useGlobalUtilityPreferencesStore.getState().save(port)).resolves.toBe(true);
    expect(port.update).toHaveBeenCalledWith("owner-1", expect.objectContaining({ isEnabled: true, revision: 1 }));
    expect(useGlobalUtilityPreferencesStore.getState().preferences.revision).toBe(2);
  });

  it("ignores a late native load after account teardown", async () => {
    let release!: (value: GlobalUtilityPreferences) => void;
    const port = stubPort({ load: vi.fn(async () => new Promise<GlobalUtilityPreferences>((resolve) => { release = resolve; })) });
    const pending = useGlobalUtilityPreferencesStore.getState().activate(port, "owner-1");
    useGlobalUtilityPreferencesStore.getState().reset();
    release(preferences());
    await pending;
    expect(useGlobalUtilityPreferencesStore.getState().ownerUserId).toBeNull();
  });

  it("reloads registration truth after a revision conflict", async () => {
    const load = vi.fn().mockResolvedValueOnce(preferences()).mockResolvedValueOnce(preferences({ revision: 2, screenshotEnabled: false }));
    const port = stubPort({ load, update: vi.fn(async () => { throw new Error("全局工具设置已经变化"); }) });
    await useGlobalUtilityPreferencesStore.getState().activate(port, "owner-1");
    useGlobalUtilityPreferencesStore.getState().updateDraft({ screenshotEnabled: false });
    await expect(useGlobalUtilityPreferencesStore.getState().save(port)).resolves.toBe(false);
    expect(useGlobalUtilityPreferencesStore.getState().preferences.revision).toBe(2);
    expect(useGlobalUtilityPreferencesStore.getState().error).toContain("已经变化");
  });
});

function stubPort(overrides: Partial<GlobalUtilityPreferencesPort> = {}): GlobalUtilityPreferencesPort {
  return {
    load: vi.fn(async () => preferences()),
    update: vi.fn(async (_owner, value) => ({ ...value, revision: value.revision + 1 })),
    deactivate: vi.fn(async () => undefined),
    ...overrides,
  };
}

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
