import { beforeEach, describe, expect, it, vi } from "vitest";
import type { GeneralSettings } from "../domain/generalSettings";
import type { GeneralSettingsPort } from "../ports/GeneralSettingsPort";
import { useGeneralSettingsStore } from "./generalSettingsStore";

describe("general settings store", () => {
  beforeEach(() => useGeneralSettingsStore.getState().reset());

  it("applies loaded presentation preferences and version-saves a draft", async () => {
    const port = stubPort();
    await useGeneralSettingsStore.getState().activate(port, "owner-1");
    useGeneralSettingsStore.getState().updateDraft({ interfaceLanguage: "en-US", interfaceFontSize: 16 });
    await expect(useGeneralSettingsStore.getState().save(port)).resolves.toBe(true);
    expect(port.update).toHaveBeenCalledWith("owner-1", expect.objectContaining({ revision: 1, interfaceLanguage: "en-US", interfaceFontSize: 16 }));
    expect(document.documentElement.lang).toBe("en-US");
    expect(document.documentElement.style.getPropertyValue("--interface-scale")).toBe(String(16 / 14));
    expect(useGeneralSettingsStore.getState().settings.revision).toBe(2);
  });

  it("rejects late load results after account teardown", async () => {
    let release!: (value: GeneralSettings) => void;
    const port = stubPort({ load: vi.fn(async () => new Promise<GeneralSettings>((resolve) => { release = resolve; })) });
    const pending = useGeneralSettingsStore.getState().activate(port, "owner-1");
    useGeneralSettingsStore.getState().reset();
    release(settings());
    await pending;
    expect(useGeneralSettingsStore.getState().ownerUserId).toBeNull();
  });

  it("re-reads persisted truth after a revision conflict", async () => {
    const load = vi.fn().mockResolvedValueOnce(settings()).mockResolvedValueOnce(settings({ revision: 2, interfaceFontSize: 15 }));
    const port = stubPort({ load, update: vi.fn(async () => { throw new Error("常规设置已经变化"); }) });
    await useGeneralSettingsStore.getState().activate(port, "owner-1");
    useGeneralSettingsStore.getState().updateDraft({ interfaceFontSize: 18 });
    await expect(useGeneralSettingsStore.getState().save(port)).resolves.toBe(false);
    expect(useGeneralSettingsStore.getState().settings.interfaceFontSize).toBe(15);
    expect(useGeneralSettingsStore.getState().error).toContain("已经变化");
  });
});

function stubPort(overrides: Partial<GeneralSettingsPort> = {}): GeneralSettingsPort {
  return {
    load: vi.fn(async () => settings()),
    update: vi.fn(async (_owner, value) => ({ ...value, revision: value.revision + 1 })),
    ...overrides,
  };
}

function settings(overrides: Partial<GeneralSettings> = {}): GeneralSettings {
  return { interfaceLanguage: "zh-CN", contextLanguage: "zh-CN", interfaceFontSize: 14, preventsIdleSystemSleep: false, revision: 1, ...overrides };
}
