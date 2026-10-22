import { beforeEach, describe, expect, it } from "vitest";
import type { LocalProject, WorkspaceRelations } from "../domain/workspaceIndex";
import type { WorkspaceIndexPort } from "../ports/WorkspaceIndexPort";
import { useWorkspaceIndexStore } from "./workspaceIndexStore";

beforeEach(() => {
  useWorkspaceIndexStore.setState({
    ownerUserId: null,
    requestId: 0,
    localProjects: [],
    relations: { contacts: [], conversations: [] },
    snapshot: { contacts: [], projects: [] },
    isLoading: false,
    errorMessage: null,
  });
});

describe("workspace index application state", () => {
  it("publishes projects before the Host refresh completes", async () => {
    const port = new TestWorkspaceIndexPort();
    const remote = deferred<WorkspaceRelations>();
    port.remoteResult = remote.promise;

    const load = useWorkspaceIndexStore.getState().load(port, "alice", true);
    await Promise.resolve();
    await Promise.resolve();

    expect(useWorkspaceIndexStore.getState().snapshot.projects[0].title).toBe("Local");
    expect(useWorkspaceIndexStore.getState().isLoading).toBe(true);
    remote.resolve({ contacts: [], conversations: [] });
    await load;
    expect(useWorkspaceIndexStore.getState().isLoading).toBe(false);
  });

  it("keeps projects when the Host relation refresh fails", async () => {
    const port = new TestWorkspaceIndexPort();
    port.remoteResult = Promise.reject(new Error("Host unavailable"));

    await useWorkspaceIndexStore.getState().load(port, "alice", true);

    expect(useWorkspaceIndexStore.getState().snapshot.projects[0].id).toBe("local");
    expect(useWorkspaceIndexStore.getState().errorMessage).toBe("Host unavailable");
  });

  it("rejects a stale result from the previous account", async () => {
    const port = new TestWorkspaceIndexPort();
    const alice = deferred<LocalProject[]>();
    port.localResults.set("alice", alice.promise);
    port.localResults.set("bob", Promise.resolve([
      { id: "bob-project", name: "Bob", revision: 1, projectTypeKey: "software_development" },
    ]));

    const oldLoad = useWorkspaceIndexStore.getState().load(port, "alice", false);
    const newLoad = useWorkspaceIndexStore.getState().load(port, "bob", false);
    await newLoad;
    alice.resolve([{ id: "alice-project", name: "Alice", revision: 1, projectTypeKey: "software_development" }]);
    await oldLoad;

    expect(useWorkspaceIndexStore.getState().ownerUserId).toBe("bob");
    expect(useWorkspaceIndexStore.getState().snapshot.projects[0].id).toBe("bob-project");
  });

  it("publishes a newly committed project only into the matching owner index", async () => {
    const port = new TestWorkspaceIndexPort();
    await useWorkspaceIndexStore.getState().load(port, "alice", false);
    useWorkspaceIndexStore.getState().acceptCreatedProject("bob", {
      id: "wrong-owner",
      name: "Wrong",
      revision: 1,
      projectTypeKey: "software_development",
    });
    expect(useWorkspaceIndexStore.getState().snapshot.projects).toHaveLength(1);
    useWorkspaceIndexStore.getState().acceptCreatedProject("alice", {
      id: "new-project",
      name: "New",
      revision: 1,
      projectTypeKey: "desktop_application",
    });
    expect(useWorkspaceIndexStore.getState().snapshot.projects.map((item) => item.id)).toEqual([
      "local",
      "new-project",
    ]);
  });

  it("removes a committed project only from the matching owner index", async () => {
    const port = new TestWorkspaceIndexPort();
    await useWorkspaceIndexStore.getState().load(port, "alice", false);

    useWorkspaceIndexStore.getState().acceptRemovedProject("bob", "local");
    expect(useWorkspaceIndexStore.getState().snapshot.projects).toHaveLength(1);
    useWorkspaceIndexStore.getState().acceptRemovedProject("alice", "local");
    expect(useWorkspaceIndexStore.getState().snapshot.projects).toEqual([]);
  });
});

class TestWorkspaceIndexPort implements WorkspaceIndexPort {
  localResults = new Map<string, Promise<LocalProject[]>>();
  remoteResult: Promise<WorkspaceRelations> = Promise.resolve({ contacts: [], conversations: [] });

  listLocalProjects(ownerUserId: string): Promise<LocalProject[]> {
    return this.localResults.get(ownerUserId) ?? Promise.resolve([
      { id: "local", name: "Local", revision: 1, projectTypeKey: "software_development" },
    ]);
  }

  refreshRelations(): Promise<WorkspaceRelations> {
    return this.remoteResult;
  }
}

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

