import { beforeEach, describe, expect, it, vi } from "vitest";
import { pluginSettingsPort, pluginSettingsSnapshot } from "../../../test/pluginSettingsFixture";
import type { PluginSettingsSnapshot } from "../domain/pluginSettings";
import { usePluginSettingsStore } from "./pluginSettingsStore";

describe("pluginSettingsStore", () => {
  beforeEach(() => usePluginSettingsStore.getState().reset());

  it("loads and mutates the native snapshot", async () => {
    const port = pluginSettingsPort();
    await usePluginSettingsStore.getState().load(port, "owner-1");
    expect(usePluginSettingsStore.getState().snapshot?.plugins[0]?.displayName).toBe("Diagram Studio");
    await usePluginSettingsStore.getState().updateEnabled(port, "plugin-1", false);
    expect(usePluginSettingsStore.getState().snapshot?.plugins[0]?.enabled).toBe(false);
    await usePluginSettingsStore.getState().install(port, "plugin-1");
    expect(usePluginSettingsStore.getState().snapshot?.plugins[0]?.installedVersion).toBe("1.2.0");
  });

  it("rejects a response that arrives after reset", async () => {
    let release!: (snapshot: PluginSettingsSnapshot) => void;
    const port = pluginSettingsPort({ load: vi.fn(async () => new Promise<PluginSettingsSnapshot>((resolve) => { release = resolve; })) });
    const pending = usePluginSettingsStore.getState().load(port, "owner-1");
    usePluginSettingsStore.getState().reset();
    release(pluginSettingsSnapshot());
    await pending;
    expect(usePluginSettingsStore.getState().snapshot).toBeNull();
  });
});
