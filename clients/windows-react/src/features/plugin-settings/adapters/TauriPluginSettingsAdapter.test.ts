import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TauriPluginSettingsAdapter } from "./TauriPluginSettingsAdapter";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("TauriPluginSettingsAdapter", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("keeps the native absolute directory path out of React state", async () => {
    vi.mocked(invoke).mockResolvedValue({
      selectionId: "selection-1",
      directoryName: "safe-plugin",
      absolutePath: "C:\\Users\\Alice\\private\\safe-plugin",
    });

    const selection = await new TauriPluginSettingsAdapter().pickManualDirectory("owner-1", "presentation-1");

    expect(selection).toEqual({ selectionId: "selection-1", directoryName: "safe-plugin" });
    expect(selection).not.toHaveProperty("absolutePath");
    expect(invoke).toHaveBeenCalledWith("pick_manual_plugin_directory", {
      ownerUserId: "owner-1",
      presentationId: "presentation-1",
    });
  });

  it("passes only the opaque selection and verified fingerprint to installation", async () => {
    vi.mocked(invoke).mockResolvedValue({
      plugins: [],
      marketplaceAvailable: true,
      localCatalogAvailable: true,
      marketplaceError: null,
      localCatalogError: null,
      secureMarketplaceInstallAvailable: true,
      manualInstallAvailable: true,
    });

    await new TauriPluginSettingsAdapter().installManualDirectory(
      "owner-1",
      "presentation-1",
      "selection-1",
      "a".repeat(64),
    );

    expect(invoke).toHaveBeenCalledWith("install_manual_plugin_directory", {
      input: {
        ownerUserId: "owner-1",
        presentationId: "presentation-1",
        selectionId: "selection-1",
        expectedFingerprint: "a".repeat(64),
      },
    });
  });
});
