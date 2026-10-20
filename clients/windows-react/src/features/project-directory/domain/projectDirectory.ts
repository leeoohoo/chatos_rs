import { z } from "zod";

const portableRelativePath = z.string().max(4_096).refine((path) => {
  if (path === "") return true;
  if (path.trim() !== path || path.includes("\\") || path.includes(":")) return false;
  return path.split("/").every((component) => (
    component.length > 0
    && component !== "."
    && component !== ".."
    && !/\p{Cc}/u.test(component)
  ));
}, "项目目录路径必须是规范化的相对路径。");

export const projectFileEntrySchema = z.object({
  name: z.string().min(1).max(1_024),
  path: portableRelativePath,
  isDirectory: z.boolean(),
  isWritable: z.boolean(),
  size: z.number().int().nonnegative().nullable(),
  modifiedAtUnixMs: z.number().int().nonnegative().nullable(),
}).strict();

export const projectDirectoryListingSchema = z.object({
  projectId: z.string().trim().min(1).max(1_024),
  path: portableRelativePath,
  parentPath: portableRelativePath.nullable(),
  isWritable: z.boolean(),
  entries: z.array(projectFileEntrySchema).max(5_000),
  isTruncated: z.boolean(),
}).strict().superRefine((listing, context) => {
  const identifiers = new Set<string>();
  listing.entries.forEach((entry, index) => {
    const expectedPrefix = listing.path === "" ? "" : `${listing.path}/`;
    if (!entry.path.startsWith(expectedPrefix) || entry.path.slice(expectedPrefix.length) !== entry.name) {
      context.addIssue({
        code: "custom",
        message: "目录项必须是当前目录的直接子项。",
        path: ["entries", index, "path"],
      });
    }
    if (identifiers.has(entry.path)) {
      context.addIssue({
        code: "custom",
        message: "目录项路径不能重复。",
        path: ["entries", index, "path"],
      });
    }
    identifiers.add(entry.path);
  });
});

export type ProjectFileEntry = z.infer<typeof projectFileEntrySchema>;
export type ProjectDirectoryListing = z.infer<typeof projectDirectoryListingSchema>;

export interface VisibleProjectEntry {
  entry: ProjectFileEntry;
  depth: number;
  expanded: boolean;
  loading: boolean;
}

export function flattenDirectoryTree(
  listings: Record<string, ProjectDirectoryListing>,
  expandedPaths: ReadonlySet<string>,
  loadingPaths: ReadonlySet<string>,
): VisibleProjectEntry[] {
  const result: VisibleProjectEntry[] = [];
  const visit = (path: string, depth: number) => {
    for (const entry of listings[path]?.entries ?? []) {
      const expanded = entry.isDirectory && expandedPaths.has(entry.path);
      result.push({ entry, depth, expanded, loading: loadingPaths.has(entry.path) });
      if (expanded) visit(entry.path, depth + 1);
    }
  };
  visit("", 0);
  return result;
}

export function displayProjectPath(path: string) {
  return path === "" ? "/" : `/${path}`;
}
