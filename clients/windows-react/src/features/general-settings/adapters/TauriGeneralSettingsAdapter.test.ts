import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { GeneralSettings } from "../domain/generalSettings";
import { TauriGeneralSettingsAdapter } from "./TauriGeneralSettingsAdapter";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("TauriGeneralSettingsAdapter", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("uses owner-scoped load and exact revisioned updates", async () => {
    const adapter = new TauriGeneralSettingsAdapter();
    vi.mocked(invoke).mockResolvedValueOnce(settings());
    await adapter.load("owner-1");
    expect(invoke).toHaveBeenLastCalledWith("load_general_settings", { ownerUserId: "owner-1" });
    vi.mocked(invoke).mockResolvedValueOnce(settings({ revision: 2, interfaceFontSize: 16 }));
    await adapter.update("owner-1", settings({ interfaceFontSize: 16 }));
    expect(invoke).toHaveBeenLastCalledWith("update_general_settings", {
      input: {
        ownerUserId: "owner-1", interfaceLanguage: "zh-CN", contextLanguage: "zh-CN",
        interfaceFontSize: 16, preventsIdleSystemSleep: false, expectedRevision: 1,
      },
    });
  });

  it("rejects unknown native preference fields", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ ...settings(), localPath: "C:\\private" });
    await expect(new TauriGeneralSettingsAdapter().load("owner-1")).rejects.toThrow();
  });
});

function settings(overrides: Partial<GeneralSettings> = {}): GeneralSettings {
  return {
    interfaceLanguage: "zh-CN", contextLanguage: "zh-CN", interfaceFontSize: 14,
    preventsIdleSystemSleep: false, revision: 1, ...overrides,
  };
}
