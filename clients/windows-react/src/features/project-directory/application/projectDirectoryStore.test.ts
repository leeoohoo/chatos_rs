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

  it("rejects a late file preview after selecting another file", async () => {
    const first = deferred<Awaited<ReturnType<ProjectDirectoryPort["readFile"]>>>();
    const port = stubPort(async () => rootListing());
    port.readFile = vi.fn(async (request) => request.path === "a.txt" ? first.promise : fileContent(request.path, "second"));
    await useProjectDirectoryStore.getState().activate(port, "owner-1", "project-1");
    const stale = useProjectDirectoryStore.getState().openFile(port, fileEntry("a.txt"));
    await useProjectDirectoryStore.getState().openFile(port, fileEntry("b.txt"));
    first.resolve(fileContent("a.txt", "first"));
    await stale;
    expect(useProjectDirectoryStore.getState().selectedFile?.path).toBe("b.txt");
    expect(useProjectDirectoryStore.getState().selectedFile?.content).toBe("second");
  });

  it("keeps partial search results and rejects a stale query", async () => {
    const first = deferred<Awaited<ReturnType<ProjectDirectoryPort["searchEntries"]>>>();
    const port = stubPort(async () => rootListing());
    port.searchEntries = vi.fn(async (request) => request.query === "old" ? first.promise : ({
      projectId: request.projectId, query: request.query, matches: [fileEntry("new.txt")],
      visitedDirectories: 1, visitedEntries: 1, isTruncated: false,
    }));
    port.searchContent = vi.fn(async (request) => ({
      projectId: request.projectId, query: request.query, matches: [], scannedFiles: 1, visitedEntries: 1, isTruncated: false,
    }));
    await useProjectDirectoryStore.getState().activate(port, "owner-1", "project-1");
    useProjectDirectoryStore.getState().setSearchText("old");
    const stale = useProjectDirectoryStore.getState().search(port, "old");
    useProjectDirectoryStore.getState().setSearchText("new");
    await useProjectDirectoryStore.getState().search(port, "new");
    first.resolve({ projectId: "project-1", query: "old", matches: [fileEntry("old.txt")], visitedDirectories: 1, visitedEntries: 1, isTruncated: false });
    await stale;
    expect(useProjectDirectoryStore.getState().searchSnapshot?.query).toBe("new");
    expect(useProjectDirectoryStore.getState().searchSnapshot?.entries[0].path).toBe("new.txt");
  });
});

type AccessLoad = (request: ProjectDirectoryLoadRequest) => Promise<ProjectDirectoryListing>;

function stubPort(load: AccessLoad): ProjectDirectoryPort {
  return {
    loadDirectory: vi.fn(load),
    readFile: vi.fn(async (request) => fileContent(request.path, "content")),
    searchEntries: vi.fn(async (request) => ({ projectId: request.projectId, query: request.query, matches: [], visitedDirectories: 1, visitedEntries: 1, isTruncated: false })),
    searchContent: vi.fn(async (request) => ({ projectId: request.projectId, query: request.query, matches: [], scannedFiles: 1, visitedEntries: 1, isTruncated: false })),
  };
}

function fileEntry(path: string) {
  return { name: path, path, isDirectory: false, isWritable: true, size: 10, modifiedAtUnixMs: 1 };
}

function fileContent(path: string, content: string) {
  return {
    projectId: "project-1", path, name: path, contentType: "text/plain", contentEncoding: "utf8" as const,
    isBinary: false, isImage: false, isWritable: true, size: content.length, modifiedAtUnixMs: 1, content,
  };
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
