import type {
  ProjectGitChange,
  ProjectGitMutationResult,
  ProjectGitSnapshot,
} from "../features/project-git/domain/projectGit";
import type { ProjectGitPort } from "../features/project-git/ports/ProjectGitPort";

export function gitChange(overrides: Partial<ProjectGitChange> = {}): ProjectGitChange {
  return {
    path: "README.md",
    originalPath: null,
    indexStatus: "M",
    workTreeStatus: " ",
    kind: "modified",
    hasStagedChanges: true,
    hasWorkingTreeChanges: false,
    ...overrides,
  };
}

export function gitSnapshot(overrides: Partial<ProjectGitSnapshot> = {}): ProjectGitSnapshot {
  return {
    projectId: "project-1",
    isRepository: true,
    currentBranch: "main",
    detachedCommit: null,
    upstream: "origin/main",
    aheadCount: 1,
    behindCount: 0,
    hasHead: true,
    changes: [
      gitChange(),
      gitChange({
        path: "Sources/App.ts",
        indexStatus: " ",
        workTreeStatus: "M",
        hasStagedChanges: false,
        hasWorkingTreeChanges: true,
      }),
    ],
    branches: [
      { name: "main", isCurrent: true, upstream: "origin/main" },
      { name: "feature/git", isCurrent: false, upstream: null },
    ],
    commits: [{
      id: "a".repeat(40),
      shortId: "a".repeat(7),
      parentIds: ["b".repeat(40)],
      author: "ChatOS Tests",
      authoredAt: "2026-10-20T12:00:00+08:00",
      decorations: ["HEAD -> main", "origin/main", "chatos/runs/private"],
      subject: "feat: project git",
    }],
    remotes: [{ name: "origin", url: "https://example.com/team/project.git", urlIsRedacted: false }],
    ...overrides,
  };
}

export function gitMutationResult(
  snapshot: ProjectGitSnapshot | null = gitSnapshot(),
): ProjectGitMutationResult {
  return {
    projectId: snapshot?.projectId ?? "project-1",
    snapshot,
    committedWithoutRefresh: snapshot == null,
  };
}

export function stubProjectGitPort(snapshot = gitSnapshot()): ProjectGitPort {
  const mutation = async () => gitMutationResult(snapshot);
  return {
    loadSnapshot: async () => snapshot,
    loadDiff: async (request) => ({
      projectId: request.projectId,
      path: request.path,
      isStaged: request.staged,
      content: "diff --git a/README.md b/README.md\n@@ -1 +1 @@\n-old\n+new",
    }),
    initializeRepository: mutation,
    stagePaths: mutation,
    unstagePaths: mutation,
    commit: mutation,
    switchBranch: mutation,
    createBranch: mutation,
    mergeBranch: mutation,
    saveRemote: mutation,
    removeRemote: mutation,
    pull: mutation,
    push: mutation,
  };
}
