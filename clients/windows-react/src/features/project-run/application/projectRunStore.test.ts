import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  projectRunAnalysis,
  projectRunState,
  runningProjectState,
  stubProjectRunPort,
} from "../../../test/projectRunFixture";
import { useProjectRunStore } from "./projectRunStore";

describe("projectRunStore", () => {
  beforeEach(() => useProjectRunStore.getState().reset());

  it("loads analysis, process state and device favorites for the exact owner/project", async () => {
    const port = stubProjectRunPort();
    port.analyze = vi.fn(port.analyze);
    port.loadFavorites = vi.fn(async () => ({ projectIds: ["project-1"] }));

    await useProjectRunStore.getState().activate(port, "owner-1", "project-1");

    const state = useProjectRunStore.getState();
    expect(state.catalog?.defaultTargetId).toBe("target-1");
    expect(state.runState?.revision).toBe(1);
    expect(state.isFavorite).toBe(true);
    expect(port.analyze).toHaveBeenCalledWith({ ownerUserId: "owner-1", projectId: "project-1" });
  });

  it("rejects a late load after switching projects", async () => {
    const first = deferred<ReturnType<typeof projectRunAnalysis>>();
    const port = stubProjectRunPort();
    port.analyze = vi.fn(async (request) => {
      if (request.projectId === "project-1") return first.promise;
      const snapshot = projectRunAnalysis();
      snapshot.catalog.projectId = "project-2";
      return snapshot;
    });
    port.loadState = vi.fn(async (request) => projectRunState({ projectId: request.projectId }));

    const oldLoad = useProjectRunStore.getState().activate(port, "owner-1", "project-1");
    await useProjectRunStore.getState().activate(port, "owner-1", "project-2");
    first.resolve(projectRunAnalysis());
    await oldLoad;

    expect(useProjectRunStore.getState().projectId).toBe("project-2");
    expect(useProjectRunStore.getState().catalog?.projectId).toBe("project-2");
  });

  it("long-polls from the latest revision and ignores results after deactivation", async () => {
    const nextPoll = deferred<{ timedOut: boolean; state: ReturnType<typeof runningProjectState> }>();
    const abandonedPoll = deferred<{ timedOut: boolean; state: ReturnType<typeof runningProjectState> }>();
    const port = stubProjectRunPort();
    port.pollChanges = vi.fn()
      .mockImplementationOnce(async () => nextPoll.promise)
      .mockImplementationOnce(async () => abandonedPoll.promise);
    await useProjectRunStore.getState().activate(port, "owner-1", "project-1");

    const monitor = useProjectRunStore.getState().monitor(port);
    expect(port.pollChanges).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      projectId: "project-1",
      afterRevision: 1,
      timeoutMs: 30_000,
    });
    nextPoll.resolve({ timedOut: false, state: runningProjectState() });
    await vi.waitFor(() => expect(useProjectRunStore.getState().runState?.revision).toBe(2));
    useProjectRunStore.getState().deactivate();
    abandonedPoll.resolve({ timedOut: false, state: runningProjectState({ revision: 3 }) });
    await monitor;

    expect(useProjectRunStore.getState().runState).toBeNull();
  });

  it("never resends an existing Secret unless the user supplies a replacement", async () => {
    const response = deferred<ReturnType<typeof projectRunAnalysis>>();
    const port = stubProjectRunPort();
    port.updateEnvironment = vi.fn(async () => response.promise);
    await useProjectRunStore.getState().activate(port, "owner-1", "project-1");

    useProjectRunStore.getState().addEnvironmentVariable();
    const drafts = useProjectRunStore.getState().environmentVariables;
    const added = drafts.at(-1)!;
    useProjectRunStore.getState().setEnvironmentVariableKey(added.id, "PORT");
    useProjectRunStore.getState().setEnvironmentVariableValue(added.id, "3000");
    const saving = useProjectRunStore.getState().saveEnvironment(port);
    expect(port.updateEnvironment).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      projectId: "project-1",
      selectedToolchains: {},
      environmentVariables: [
        { key: "API_TOKEN", value: null },
        { key: "PORT", value: "3000" },
      ],
    });

    useProjectRunStore.getState().setEnvironmentVariableValue(added.id, "4000");
    response.resolve(projectRunAnalysis());
    await saving;
    expect(useProjectRunStore.getState().environmentVariables.find((item) => item.id === added.id)?.value).toBe("4000");
  });

  it("reconciles the latest process state after an ambiguous mutation failure", async () => {
    const port = stubProjectRunPort();
    port.start = vi.fn(async () => { throw new Error("IPC 写回中断"); });
    port.loadState = vi.fn()
      .mockResolvedValueOnce(projectRunState())
      .mockResolvedValueOnce(runningProjectState());
    await useProjectRunStore.getState().activate(port, "owner-1", "project-1");

    await useProjectRunStore.getState().start(port);

    expect(useProjectRunStore.getState().runState?.instances).toHaveLength(1);
    expect(useProjectRunStore.getState().errorMessage).toBe("IPC 写回中断");
  });

  it("removes only the ChatOS project record and reports success to the shell", async () => {
    const port = stubProjectRunPort();
    port.removeProject = vi.fn(async () => ({
      projectId: "project-1",
      removedRevision: 4,
      registryRevision: 5,
      directoryPreserved: true as const,
      preferencesCleaned: true,
    }));
    await useProjectRunStore.getState().activate(port, "owner-1", "project-1");

    await expect(useProjectRunStore.getState().removeProject(port)).resolves.toBe(true);
    expect(port.removeProject).toHaveBeenCalledWith({ ownerUserId: "owner-1", projectId: "project-1" });
  });
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}
