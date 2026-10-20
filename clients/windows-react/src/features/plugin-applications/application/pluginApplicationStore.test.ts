import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  pluginApplicationFixture,
  pluginApplicationLaunchFixture,
} from "../../../test/pluginApplicationFixture";
import type { PluginApplicationLaunch } from "../domain/pluginApplications";
import type { PluginApplicationPort } from "../ports/PluginApplicationPort";
import { usePluginApplicationStore } from "./pluginApplicationStore";

describe("plugin application store", () => {
  beforeEach(() => usePluginApplicationStore.getState().reset());

  it("rejects a late catalog result after the account changes", async () => {
    const alice = deferred<ReturnType<PluginApplicationPort["list"]> extends Promise<infer T> ? T : never>();
    const bob = deferred<ReturnType<PluginApplicationPort["list"]> extends Promise<infer T> ? T : never>();
    const adapter = port({ list: vi.fn((owner) => owner === "alice" ? alice.promise : bob.promise) });
    usePluginApplicationStore.getState().activateCatalog(adapter, "alice", true);
    usePluginApplicationStore.getState().activateCatalog(adapter, "bob", true);
    alice.resolve([pluginApplicationFixture({ displayName: "Alice App" })]);
    await Promise.resolve();
    expect(usePluginApplicationStore.getState().applications).toEqual([]);
    bob.resolve([pluginApplicationFixture({ displayName: "Bob App" })]);
    await vi.waitFor(() => expect(usePluginApplicationStore.getState().applications[0]?.displayName).toBe("Bob App"));
  });

  it("uses latest-wins generations when the selected project changes", async () => {
    const first = deferred<PluginApplicationLaunch>();
    const second = deferred<PluginApplicationLaunch>();
    const launch = vi.fn()
      .mockImplementationOnce(() => first.promise)
      .mockImplementationOnce(() => second.promise);
    const adapter = port({ launch });
    const application = pluginApplicationFixture();
    usePluginApplicationStore.getState().activateApplication(adapter, "alice", true, application);
    usePluginApplicationStore.getState().selectContext(adapter, { kind: "project", projectId: "project-1" });
    usePluginApplicationStore.getState().chooseAnotherContext(adapter);
    usePluginApplicationStore.getState().selectContext(adapter, { kind: "project", projectId: "project-2" });

    expect(launch.mock.calls.map((call) => call[0].generation)).toEqual([1, 2]);
    first.resolve(pluginApplicationLaunchFixture());
    await Promise.resolve();
    expect(usePluginApplicationStore.getState().phase).toBe("launching");
    second.resolve(pluginApplicationLaunchFixture({
      context: { projectId: "project-2", projectName: "Project Two" },
    }));
    await vi.waitFor(() => expect(usePluginApplicationStore.getState().phase).toBe("ready"));
    expect(usePluginApplicationStore.getState().launch?.context?.projectId).toBe("project-2");
    expect(adapter.discardPresentation).toHaveBeenCalledWith(expect.objectContaining({ generation: 1 }));
  });

  it("discards the presentation and ignores a launch completed after page teardown", async () => {
    const late = deferred<PluginApplicationLaunch>();
    const adapter = port({ launch: vi.fn(() => late.promise) });
    const application = pluginApplicationFixture({ contextScope: null, missingContext: null });
    usePluginApplicationStore.getState().activateApplication(adapter, "alice", true, application);
    const presentationId = usePluginApplicationStore.getState().presentationId;
    usePluginApplicationStore.getState().deactivateApplication(
      adapter,
      "alice",
      "com.example.canvas:canvas-workbench",
    );
    late.resolve(pluginApplicationLaunchFixture({ application, context: null }));
    await Promise.resolve();
    expect(usePluginApplicationStore.getState().phase).toBe("inactive");
    expect(usePluginApplicationStore.getState().launch).toBeNull();
    await vi.waitFor(() => expect(adapter.discardPresentation).toHaveBeenCalledWith({
      ownerUserId: "alice",
      presentationId,
      generation: 1,
    }));
  });
});

function port(overrides: Partial<PluginApplicationPort> = {}): PluginApplicationPort {
  return {
    list: vi.fn(async () => []),
    launch: vi.fn(async () => pluginApplicationLaunchFixture()),
    discardPresentation: vi.fn(async () => undefined),
    handleBridgeRequest: vi.fn(async (request) => ({ requestId: request.requestId, result: {} })),
    ...overrides,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((accept) => { resolve = accept; });
  return { promise, resolve };
}
