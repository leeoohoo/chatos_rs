import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjectDirectoryListing } from "../domain/projectDirectory";
import type { ProjectDirectoryLoadRequest, ProjectDirectoryPort } from "../ports/ProjectDirectoryPort";
import { useProjectDirectoryStore } from "./projectDirectoryStore";

describe("project directory store", () => {
  beforeEach(() => useProjectDirectoryStore.getState().reset());

  it("loads the root then expands a directory", async () => {
    const port = stubPort(async (request) => request.path === "" ? rootListing() : childListing());
    await useProjectDirectoryStore.getState().activate(port, "owner-1", "project-1");
    await useProjectDirectoryStore.getState().toggle(port, rootListing().entries[0]);
    expect(useProjectDirectoryStore.getState().listings.Sources).toEqual(childListing());
    expect(useProjectDirectoryStore.getState().expandedPaths.has("Sources")).toBe(true);
  });

  it("rejects a late root after project replacement", async () => {
    const first = deferred<ProjectDirectoryListing>();
    const port = stubPort(async (request) => {
      if (request.projectId === "project-a") return first.promise;
      return { ...rootListing(), projectId: "project-b" };
    });
    const stale = useProjectDirectoryStore.getState().activate(port, "owner-1", "project-a");
    await useProjectDirectoryStore.getState().activate(port, "owner-1", "project-b");
    first.resolve({ ...rootListing(), projectId: "project-a" });
    await stale;
    expect(useProjectDirectoryStore.getState().projectId).toBe("project-b");
    expect(useProjectDirectoryStore.getState().listings[""]?.projectId).toBe("project-b");
  });

  it("rejects a late child after its directory collapses", async () => {
    const child = deferred<ProjectDirectoryListing>();
    const port = stubPort(async (request) => request.path === "" ? rootListing() : child.promise);
    await useProjectDirectoryStore.getState().activate(port, "owner-1", "project-1");
    const directory = rootListing().entries[0];
    const expansion = useProjectDirectoryStore.getState().toggle(port, directory);
    await useProjectDirectoryStore.getState().toggle(port, directory);
    child.resolve(childListing());
    await expansion;
    expect(useProjectDirectoryStore.getState().expandedPaths.has("Sources")).toBe(false);
    expect(useProjectDirectoryStore.getState().listings.Sources).toBeUndefined();
  });

  it("preserves the last trusted tree when refresh fails", async () => {
    const load = vi.fn<AccessLoad>()
      .mockResolvedValueOnce(rootListing())
      .mockRejectedValueOnce(new Error("directory offline"));
    const port = stubPort(load);
    await useProjectDirectoryStore.getState().activate(port, "owner-1", "project-1");
    await useProjectDirectoryStore.getState().refresh(port);
    expect(useProjectDirectoryStore.getState().listings[""]).toEqual(rootListing());
    expect(useProjectDirectoryStore.getState().error).toBe("directory offline");
  });
});

type AccessLoad = (request: ProjectDirectoryLoadRequest) => Promise<ProjectDirectoryListing>;

function stubPort(load: AccessLoad): ProjectDirectoryPort {
  return { loadDirectory: vi.fn(load) };
}

function rootListing(): ProjectDirectoryListing {
  return {
    projectId: "project-1",
    path: "",
    parentPath: null,
    isWritable: true,
    entries: [{
      name: "Sources",
      path: "Sources",
      isDirectory: true,
      isWritable: true,
      size: null,
      modifiedAtUnixMs: 1,
    }],
    isTruncated: false,
  };
}

function childListing(): ProjectDirectoryListing {
  return {
    projectId: "project-1",
    path: "Sources",
    parentPath: "",
    isWritable: true,
    entries: [{
      name: "App.ts",
      path: "Sources/App.ts",
      isDirectory: false,
      isWritable: true,
      size: 10,
      modifiedAtUnixMs: 1,
    }],
    isTruncated: false,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((accept) => { resolve = accept; });
  return { promise, resolve };
}
