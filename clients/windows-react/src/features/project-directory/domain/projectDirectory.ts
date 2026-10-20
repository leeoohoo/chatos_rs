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

export const projectFileContentSchema = z.object({
  projectId: z.string().trim().min(1).max(1_024),
  path: portableRelativePath.refine((path) => path.length > 0),
  name: z.string().min(1).max(1_024),
  contentType: z.string().min(1).max(256),
  contentEncoding: z.enum(["utf8", "base64"]),
  isBinary: z.boolean(),
  isImage: z.boolean(),
  isWritable: z.boolean(),
  size: z.number().int().nonnegative().max(25 * 1_024 * 1_024),
  modifiedAtUnixMs: z.number().int().nonnegative().nullable(),
  revision: z.string().regex(/^[0-9a-f]{64}$/),
  content: z.string().max(36 * 1_024 * 1_024),
}).strict().superRefine((file, context) => {
  if (file.isBinary !== (file.contentEncoding === "base64")) {
    context.addIssue({ code: "custom", message: "文件编码与二进制标记不一致。", path: ["contentEncoding"] });
  }
});

export const projectEntrySearchResultSchema = z.object({
  projectId: z.string().trim().min(1).max(1_024),
  query: z.string().trim().min(1).max(1_024),
  matches: z.array(projectFileEntrySchema).max(500),
  visitedDirectories: z.number().int().nonnegative().max(20_000),
  visitedEntries: z.number().int().nonnegative().max(20_000),
  isTruncated: z.boolean(),
}).strict();

export const projectContentMatchSchema = z.object({
  path: portableRelativePath.refine((path) => path.length > 0),
  line: z.number().int().positive(),
  column: z.number().int().positive(),
  text: z.string().max(2_000),
}).strict();

export const projectContentSearchResultSchema = z.object({
  projectId: z.string().trim().min(1).max(1_024),
  query: z.string().trim().min(1).max(1_024),
  matches: z.array(projectContentMatchSchema).max(500),
  scannedFiles: z.number().int().nonnegative().max(20_000),
  visitedEntries: z.number().int().nonnegative().max(20_000),
  isTruncated: z.boolean(),
}).strict();

export const projectFileWriteResultSchema = z.object({
  projectId: z.string().trim().min(1).max(1_024),
  path: portableRelativePath.refine((path) => path.length > 0),
  file: projectFileContentSchema.nullable(),
}).strict();

export const projectEntryMutationResultSchema = z.object({
  projectId: z.string().trim().min(1).max(1_024),
  path: portableRelativePath.refine((path) => path.length > 0),
  parentPath: portableRelativePath,
  listing: projectDirectoryListingSchema.nullable(),
}).strict();

export type ProjectFileContent = z.infer<typeof projectFileContentSchema>;
export type ProjectEntrySearchResult = z.infer<typeof projectEntrySearchResultSchema>;
export type ProjectContentMatch = z.infer<typeof projectContentMatchSchema>;
export type ProjectContentSearchResult = z.infer<typeof projectContentSearchResultSchema>;
export type ProjectFileWriteResult = z.infer<typeof projectFileWriteResultSchema>;
export type ProjectEntryMutationResult = z.infer<typeof projectEntryMutationResultSchema>;

export interface ProjectSearchSnapshot {
  query: string;
  entries: ProjectFileEntry[];
  content: ProjectContentMatch[];
  entryTruncated: boolean;
  contentTruncated: boolean;
}

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

export function fileNameFromPath(path: string) {
  return path.split("/").at(-1) ?? path;
}

export function projectImageDataUrl(file: ProjectFileContent) {
  if (!file.isImage) return null;
  if (file.contentEncoding === "base64") return `data:${file.contentType};base64,${file.content}`;
  return `data:${file.contentType};charset=utf-8,${encodeURIComponent(file.content)}`;
}
