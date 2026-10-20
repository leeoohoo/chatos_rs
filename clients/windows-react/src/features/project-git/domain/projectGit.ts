import { z } from "zod";

const safeText = (maximum: number) => z.string().max(maximum).refine(
  (value) => !/\p{Cc}/u.test(value),
  "字段包含控制字符。",
);

const projectIdentifierSchema = safeText(1_024).min(1).refine(
  (value) => value.trim() === value,
  "项目标识不能包含首尾空白。",
);

export const projectGitPathSchema = safeText(16 * 1_024).min(1).refine(
  (value) => value.trim() === value
    && !value.startsWith("/")
    && !value.includes("\\")
    && !value.includes(":")
    && value.split("/").every((component) => component.length > 0 && component !== "." && component !== ".."),
  "Git 路径必须是规范化的项目相对路径。",
);

export const projectGitChangeKindSchema = z.enum([
  "modified",
  "added",
  "deleted",
  "renamed",
  "copied",
  "untracked",
  "conflicted",
  "typeChanged",
]);

const gitStatusSchema = z.string().length(1).regex(/^[ MADRCUT?!]$/);

export const projectGitChangeSchema = z.object({
  path: projectGitPathSchema,
  originalPath: projectGitPathSchema.nullable(),
  indexStatus: gitStatusSchema,
  workTreeStatus: gitStatusSchema,
  kind: projectGitChangeKindSchema,
  hasStagedChanges: z.boolean(),
  hasWorkingTreeChanges: z.boolean(),
}).superRefine((change, context) => {
  const staged = change.indexStatus !== " " && change.indexStatus !== "?";
  const working = change.workTreeStatus !== " ";
  if (change.hasStagedChanges !== staged || change.hasWorkingTreeChanges !== working) {
    context.addIssue({ code: "custom", message: "Git 变更状态与派生状态不一致。" });
  }
});

export const projectGitBranchSchema = z.object({
  name: safeText(1_024).min(1),
  isCurrent: z.boolean(),
  upstream: safeText(64 * 1_024).min(1).nullable(),
});

const objectIdSchema = z.string().regex(/^[0-9a-fA-F]{7,64}$/);

export const projectGitCommitSchema = z.object({
  id: objectIdSchema,
  shortId: objectIdSchema,
  parentIds: z.array(objectIdSchema),
  author: safeText(64 * 1_024),
  authoredAt: safeText(128).min(1).nullable(),
  decorations: z.array(safeText(64 * 1_024).min(1)).max(64),
  subject: safeText(64 * 1_024),
});

export const projectGitRemoteSchema = z.object({
  name: safeText(1_024).min(1),
  url: safeText(16 * 1_024),
  urlIsRedacted: z.boolean(),
});

export const projectGitSnapshotSchema = z.object({
  projectId: projectIdentifierSchema,
  isRepository: z.boolean(),
  currentBranch: safeText(1_024).min(1).nullable(),
  detachedCommit: objectIdSchema.nullable(),
  upstream: safeText(64 * 1_024).min(1).nullable(),
  aheadCount: z.number().int().nonnegative().safe(),
  behindCount: z.number().int().nonnegative().safe(),
  hasHead: z.boolean(),
  changes: z.array(projectGitChangeSchema),
  branches: z.array(projectGitBranchSchema),
  commits: z.array(projectGitCommitSchema).max(80),
  remotes: z.array(projectGitRemoteSchema),
}).superRefine((snapshot, context) => {
  if (!snapshot.isRepository && (snapshot.hasHead
    || snapshot.currentBranch != null
    || snapshot.detachedCommit != null
    || snapshot.upstream != null
    || snapshot.aheadCount > 0
    || snapshot.behindCount > 0
    || snapshot.changes.length > 0
    || snapshot.branches.length > 0
    || snapshot.commits.length > 0
    || snapshot.remotes.length > 0)) {
    context.addIssue({ code: "custom", message: "非 Git 项目不能携带仓库状态。" });
  }
});

export const projectGitDiffSchema = z.object({
  projectId: projectIdentifierSchema,
  path: projectGitPathSchema,
  isStaged: z.boolean(),
  content: z.string().max(4 * 1_024 * 1_024),
});

export const projectGitMutationResultSchema = z.object({
  projectId: projectIdentifierSchema,
  snapshot: projectGitSnapshotSchema.nullable(),
  committedWithoutRefresh: z.boolean(),
}).superRefine((result, context) => {
  if (result.committedWithoutRefresh !== (result.snapshot == null)) {
    context.addIssue({ code: "custom", message: "Git mutation 刷新状态不一致。" });
  }
  if (result.snapshot && result.snapshot.projectId !== result.projectId) {
    context.addIssue({ code: "custom", message: "Git mutation 返回了其他项目的状态。" });
  }
});

export type ProjectGitChange = z.infer<typeof projectGitChangeSchema>;
export type ProjectGitBranch = z.infer<typeof projectGitBranchSchema>;
export type ProjectGitCommit = z.infer<typeof projectGitCommitSchema>;
export type ProjectGitRemote = z.infer<typeof projectGitRemoteSchema>;
export type ProjectGitSnapshot = z.infer<typeof projectGitSnapshotSchema>;
export type ProjectGitDiff = z.infer<typeof projectGitDiffSchema>;
export type ProjectGitMutationResult = z.infer<typeof projectGitMutationResultSchema>;

export interface ProjectGitDiffLine {
  id: number;
  oldLine: number | null;
  newLine: number | null;
  marker: string;
  content: string;
  kind: "header" | "hunk" | "context" | "addition" | "deletion";
}

export function unavailableProjectGitSnapshot(projectId: string): ProjectGitSnapshot {
  return {
    projectId,
    isRepository: false,
    currentBranch: null,
    detachedCommit: null,
    upstream: null,
    aheadCount: 0,
    behindCount: 0,
    hasHead: false,
    changes: [],
    branches: [],
    commits: [],
    remotes: [],
  };
}

export function stagedProjectGitChanges(snapshot: ProjectGitSnapshot) {
  return snapshot.changes.filter((change) => change.hasStagedChanges);
}

export function workingTreeProjectGitChanges(snapshot: ProjectGitSnapshot) {
  return snapshot.changes.filter((change) => change.hasWorkingTreeChanges);
}

export function visibleGitDecorations(commit: ProjectGitCommit) {
  return commit.decorations.flatMap((decoration) => {
    if (decoration.startsWith("chatos/runs/") || decoration.startsWith("chatos/executions/")) return [];
    if (decoration.startsWith("HEAD -> ")) return [decoration.slice("HEAD -> ".length)];
    if (decoration.startsWith("tag: ")) return [decoration.slice("tag: ".length)];
    return [decoration];
  }).slice(0, 3);
}

export function parseProjectGitDiff(content: string): ProjectGitDiffLine[] {
  let oldLine = 0;
  let newLine = 0;
  return content.split("\n").slice(0, 8_000).map((value, id) => {
    if (value.startsWith("@@")) {
      const starts = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(value);
      if (starts) {
        oldLine = Number(starts[1]);
        newLine = Number(starts[2]);
      }
      return { id, oldLine: null, newLine: null, marker: "@@", content: value, kind: "hunk" };
    }
    if (["+++", "---", "diff ", "index "].some((prefix) => value.startsWith(prefix))) {
      return { id, oldLine: null, newLine: null, marker: "", content: value, kind: "header" };
    }
    if (value.startsWith("+")) {
      const line = { id, oldLine: null, newLine, marker: "+", content: value.slice(1), kind: "addition" } as const;
      newLine += 1;
      return line;
    }
    if (value.startsWith("-")) {
      const line = { id, oldLine, newLine: null, marker: "−", content: value.slice(1), kind: "deletion" } as const;
      oldLine += 1;
      return line;
    }
    if (value.startsWith(" ")) {
      const line = { id, oldLine, newLine, marker: "", content: value.slice(1), kind: "context" } as const;
      oldLine += 1;
      newLine += 1;
      return line;
    }
    return { id, oldLine: null, newLine: null, marker: "", content: value, kind: "header" };
  });
}

export function projectGitFileName(path: string) {
  return path.split("/").at(-1) ?? path;
}

export function projectGitParentPath(path: string) {
  const separator = path.lastIndexOf("/");
  return separator < 0 ? "" : path.slice(0, separator);
}
