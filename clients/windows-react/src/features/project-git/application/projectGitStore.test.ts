import { beforeEach, describe, expect, it, vi } from "vitest";
import { gitChange, gitMutationResult, gitSnapshot, stubProjectGitPort } from "../../../test/projectGitFixture";
import type { ProjectGitDiff, ProjectGitMutationResult, ProjectGitSnapshot } from "../domain/projectGit";
import type { ProjectGitPort } from "../ports/ProjectGitPort";
import { useProjectGitStore } from "./projectGitStore";

describe("project Git store lifecycle", () => {
  beforeEach(() => useProjectGitStore.getState().reset());

  it("rejects a late snapshot after switching projects", async () => {
    const first = deferred<ProjectGitSnapshot>();
    const port = stubProjectGitPort();
    port.loadSnapshot = vi.fn(async (request) => request.projectId === "project-1"
      ? first.promise
      : gitSnapshot({ projectId: "project-2", currentBranch: "project-two" }));

    const oldLoad = useProjectGitStore.getState().activate(port, "owner-1", "project-1");
    await useProjectGitStore.getState().activate(port, "owner-1", "project-2");
    first.resolve(gitSnapshot({ currentBranch: "late-old" }));
    await oldLoad;

    expect(useProjectGitStore.getState().projectId).toBe("project-2");
    expect(useProjectGitStore.getState().snapshot.currentBranch).toBe("project-two");
  });

  it("keeps the newest diff when an older request completes late", async () => {
    const first = deferred<ProjectGitDiff>();
    const second = deferred<ProjectGitDiff>();
    const port = stubProjectGitPort();
    port.loadDiff = vi.fn(async (request) => request.path === "README.md" ? first.promise : second.promise);
    await activate(port);

    const oldRequest = useProjectGitStore.getState().showDiff(port, gitChange(), true);
    const newestRequest = useProjectGitStore.getState().showDiff(
      port,
      gitChange({ path: "Sources/App.ts", indexStatus: " ", workTreeStatus: "M", hasStagedChanges: false, hasWorkingTreeChanges: true }),
      false,
    );
    second.resolve({ projectId: "project-1", path: "Sources/App.ts", isStaged: false, content: "+new" });
    await newestRequest;
    first.resolve({ projectId: "project-1", path: "README.md", isStaged: true, content: "+old" });
    await oldRequest;

    expect(useProjectGitStore.getState().selectedDiff?.path).toBe("Sources/App.ts");
  });

  it("invalidates load and diff requests when a mutation starts", async () => {
    const load = deferred<ProjectGitSnapshot>();
    const diff = deferred<ProjectGitDiff>();
    const port = stubProjectGitPort();
    await activate(port);
    port.loadSnapshot = vi.fn(async () => load.promise);
    port.loadDiff = vi.fn(async () => diff.promise);
    port.stagePaths = vi.fn(async () => gitMutationResult(null));

    const lateLoad = useProjectGitStore.getState().load(port);
    const lateDiff = useProjectGitStore.getState().showDiff(port, gitChange(), true);
    await useProjectGitStore.getState().stage(port, gitChange());
    load.resolve(gitSnapshot({ currentBranch: "stale" }));
    diff.resolve({ projectId: "project-1", path: "README.md", isStaged: true, content: "stale" });
    await Promise.all([lateLoad, lateDiff]);

    const state = useProjectGitStore.getState();
    expect(state.snapshot.currentBranch).toBe("main");
    expect(state.selectedDiff).toBeNull();
    expect(state.errorMessage).toContain("已完成");
  });

  it("does not clear a commit message typed while an older commit is running", async () => {
    const result = deferred<ProjectGitMutationResult>();
    const port = stubProjectGitPort();
    port.commit = vi.fn(async () => result.promise);
    await activate(port);
    useProjectGitStore.getState().setCommitMessage("first message");

    const commit = useProjectGitStore.getState().commit(port);
    useProjectGitStore.getState().setCommitMessage("next message");
    result.resolve(gitMutationResult(gitSnapshot({ changes: [] })));
    await commit;

    expect(useProjectGitStore.getState().commitMessage).toBe("next message");
  });

  it("recovers the latest snapshot after failure while preserving the operation error", async () => {
    const recovered = gitSnapshot({ currentBranch: "recovered" });
    const port = stubProjectGitPort();
    await activate(port);
    port.stagePaths = vi.fn(async () => { throw new Error("stage failed"); });
    port.loadSnapshot = vi.fn(async () => recovered);

    await useProjectGitStore.getState().stage(port, gitChange());

    expect(useProjectGitStore.getState().snapshot.currentBranch).toBe("recovered");
    expect(useProjectGitStore.getState().errorMessage).toBe("stage failed");
  });

  it("ignores a completed side effect after the user switches projects", async () => {
    const result = deferred<ProjectGitMutationResult>();
    const port = stubProjectGitPort();
    await activate(port);
    port.switchBranch = vi.fn(async () => result.promise);
    port.loadSnapshot = vi.fn(async (request) => gitSnapshot({ projectId: request.projectId, currentBranch: "project-two" }));

    const mutation = useProjectGitStore.getState().switchBranch(port, "feature/git");
    await useProjectGitStore.getState().activate(port, "owner-1", "project-2");
    result.resolve(gitMutationResult(gitSnapshot({ currentBranch: "late-feature" })));
    await mutation;

    expect(useProjectGitStore.getState().projectId).toBe("project-2");
    expect(useProjectGitStore.getState().snapshot.currentBranch).toBe("project-two");
  });
});

async function activate(port: ProjectGitPort) {
  await useProjectGitStore.getState().activate(port, "owner-1", "project-1");
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}
