import { beforeEach, describe, expect, it, vi } from "vitest";
import { pluginSettingsPort, pluginSettingsSnapshot } from "../../../test/pluginSettingsFixture";
import type { ManualPluginDirectorySelection, ManualPluginInspection, PluginSettingsSnapshot } from "../domain/pluginSettings";
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

  it("invalidates the previous inspection when a new directory is selected", async () => {
    let selection = 0;
    const port = pluginSettingsPort({
      pickManualDirectory: vi.fn(async () => {
        selection += 1;
        return { selectionId: `selection-${selection}`, directoryName: `plugin-${selection}` };
      }),
    });
    await usePluginSettingsStore.getState().load(port, "owner-1");
    await usePluginSettingsStore.getState().pickManualDirectory(port, "presentation-1");
    await usePluginSettingsStore.getState().inspectManualDirectory(port, "presentation-1");
    expect(usePluginSettingsStore.getState().manualInspection?.selectionId).toBe("selection-1");

    await usePluginSettingsStore.getState().pickManualDirectory(port, "presentation-1");
    expect(usePluginSettingsStore.getState().manualSelection?.selectionId).toBe("selection-2");
    expect(usePluginSettingsStore.getState().manualInspection).toBeNull();
  });

  it("keeps the current preflight when the native folder picker is cancelled", async () => {
    let cancelled = false;
    const port = pluginSettingsPort({
      pickManualDirectory: vi.fn(async () => cancelled
        ? null
        : { selectionId: "selection-1", directoryName: "plugin-1" }),
    });
    await usePluginSettingsStore.getState().load(port, "owner-1");
    await usePluginSettingsStore.getState().pickManualDirectory(port, "presentation-1");
    await usePluginSettingsStore.getState().inspectManualDirectory(port, "presentation-1");
    cancelled = true;

    expect(await usePluginSettingsStore.getState().pickManualDirectory(port, "presentation-1")).toBe(false);
    expect(usePluginSettingsStore.getState().manualSelection?.selectionId).toBe("selection-1");
    expect(usePluginSettingsStore.getState().manualInspection?.selectionId).toBe("selection-1");
  });

  it("requires a current preflight before manual installation and refreshes the snapshot", async () => {
    const installedSnapshot = pluginSettingsSnapshot();
    installedSnapshot.plugins[0]!.installedVersion = "1.2.0";
    const port = pluginSettingsPort({ installManualDirectory: vi.fn(async () => installedSnapshot) });
    await usePluginSettingsStore.getState().load(port, "owner-1");
    await usePluginSettingsStore.getState().pickManualDirectory(port, "presentation-1");
    expect(await usePluginSettingsStore.getState().installManualDirectory(port, "presentation-1")).toBe(false);
    expect(port.installManualDirectory).not.toHaveBeenCalled();

    await usePluginSettingsStore.getState().inspectManualDirectory(port, "presentation-1");
    expect(await usePluginSettingsStore.getState().installManualDirectory(port, "presentation-1")).toBe(true);
    expect(usePluginSettingsStore.getState().snapshot?.plugins[0]?.installedVersion).toBe("1.2.0");
    expect(usePluginSettingsStore.getState().manualSelection).toBeNull();
    expect(usePluginSettingsStore.getState().manualInspection).toBeNull();
  });

  it("ignores a picker response that arrives after the presentation is discarded", async () => {
    let release!: (selection: { selectionId: string; directoryName: string }) => void;
    const port = pluginSettingsPort({
      pickManualDirectory: vi.fn(async () => new Promise<ManualPluginDirectorySelection>((resolve) => { release = resolve; })),
      discardManualDirectory: vi.fn(async () => undefined),
    });
    await usePluginSettingsStore.getState().load(port, "owner-1");
    const pending = usePluginSettingsStore.getState().pickManualDirectory(port, "presentation-1");
    await usePluginSettingsStore.getState().discardManualDirectory(port, "presentation-1");
    release({ selectionId: "late-selection", directoryName: "late-plugin" });

    expect(await pending).toBe(false);
    expect(usePluginSettingsStore.getState().manualSelection).toBeNull();
    expect(usePluginSettingsStore.getState().manualBusy).toBeNull();
    expect(port.discardManualDirectory).toHaveBeenCalledWith("owner-1", "presentation-1");
  });

  it("ignores a preflight failure that arrives after reset", async () => {
    let reject!: (failure: Error) => void;
    const port = pluginSettingsPort({
      inspectManualDirectory: vi.fn(async () => new Promise<ManualPluginInspection>((_resolve, rejectPromise) => { reject = rejectPromise; })),
    });
    await usePluginSettingsStore.getState().load(port, "owner-1");
    await usePluginSettingsStore.getState().pickManualDirectory(port, "presentation-1");
    const pending = usePluginSettingsStore.getState().inspectManualDirectory(port, "presentation-1");
    usePluginSettingsStore.getState().reset();
    reject(new Error("late failure"));

    expect(await pending).toBe(false);
    expect(usePluginSettingsStore.getState().error).toBeNull();
    expect(usePluginSettingsStore.getState().manualInspection).toBeNull();
  });
});
