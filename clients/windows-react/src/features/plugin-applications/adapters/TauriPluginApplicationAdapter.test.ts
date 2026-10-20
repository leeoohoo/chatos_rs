import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  pluginApplicationFixture,
  pluginApplicationLaunchFixture,
} from "../../../test/pluginApplicationFixture";
import { TauriPluginApplicationAdapter } from "./TauriPluginApplicationAdapter";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("TauriPluginApplicationAdapter", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("passes only the typed launch request and parses the native response", async () => {
    vi.mocked(invoke).mockResolvedValue(pluginApplicationLaunchFixture());
    const request = {
      ownerUserId: "alice",
      presentationId: "plugin-view-1",
      generation: 1,
      pluginId: "com.example.canvas",
      componentKey: "canvas-workbench",
      context: { kind: "project" as const, projectId: "project-1" },
    };
    await new TauriPluginApplicationAdapter().launch(request);
    expect(invoke).toHaveBeenCalledWith("launch_plugin_application", { request });
  });

  it("rejects catalog records that expose native paths", async () => {
    vi.mocked(invoke).mockResolvedValue([{ ...pluginApplicationFixture(), installationRoot: "C:\\Plugins" }]);
    await expect(new TauriPluginApplicationAdapter().list("alice")).rejects.toThrow();
  });
});
