import { beforeEach, describe, expect, it, vi } from "vitest";
import type { PetPreferences } from "../domain/petPreferences";
import type { PetPreferencesPort } from "../ports/PetPreferencesPort";
import { usePetPreferencesStore } from "./petPreferencesStore";

describe("pet preferences store", () => {
  beforeEach(() => usePetPreferencesStore.getState().reset());

  it("version-saves device preferences and resets native position", async () => {
    const port = stubPort();
    await usePetPreferencesStore.getState().activate(port, "owner-1");
    usePetPreferencesStore.getState().updateDraft({ isEnabled: false, size: 180 });
    await expect(usePetPreferencesStore.getState().save(port)).resolves.toBe(true);
    expect(port.update).toHaveBeenCalledWith("owner-1", expect.objectContaining({ isEnabled: false, size: 180, revision: 1 }));
    await expect(usePetPreferencesStore.getState().resetPosition(port)).resolves.toBe(true);
    expect(port.resetPosition).toHaveBeenCalledWith("owner-1", 2);
  });

  it("ignores a late load after account teardown", async () => {
    let release!: (value: PetPreferences) => void;
    const port = stubPort({ load: vi.fn(async () => new Promise<PetPreferences>((resolve) => { release = resolve; })) });
    const pending = usePetPreferencesStore.getState().activate(port, "owner-1");
    usePetPreferencesStore.getState().reset();
    release(preferences());
    await pending;
    expect(usePetPreferencesStore.getState().ownerUserId).toBeNull();
  });

  it("reloads native truth after a revision conflict", async () => {
    const load = vi.fn().mockResolvedValueOnce(preferences()).mockResolvedValueOnce(preferences({ revision: 2, size: 120 }));
    const port = stubPort({ load, update: vi.fn(async () => { throw new Error("宠物设置已经变化"); }) });
    await usePetPreferencesStore.getState().activate(port, "owner-1");
    usePetPreferencesStore.getState().updateDraft({ size: 180 });
    await expect(usePetPreferencesStore.getState().save(port)).resolves.toBe(false);
    expect(usePetPreferencesStore.getState().preferences.size).toBe(120);
    expect(usePetPreferencesStore.getState().error).toContain("已经变化");
  });
});

function stubPort(overrides: Partial<PetPreferencesPort> = {}): PetPreferencesPort {
  return {
    load: vi.fn(async () => preferences()),
    update: vi.fn(async (_owner, value) => ({ ...value, revision: value.revision + 1 })),
    resetPosition: vi.fn(async (_owner, revision) => preferences({ revision: revision + 1 })),
    ...overrides,
  };
}

function preferences(overrides: Partial<PetPreferences> = {}): PetPreferences {
  return { isEnabled: true, size: 104, showProcess: true, showCompletions: true, showAcrossSpaces: true, revision: 1, ...overrides };
}
