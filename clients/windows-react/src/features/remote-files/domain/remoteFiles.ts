import { z } from "zod";

const safeText = (maximum: number) => z.string().min(1).max(maximum).refine(
  (value) => !/[\u0000-\u001f\u007f]/u.test(value),
  "远端文件字段包含不安全的控制字符。",
);
const identifier = safeText(128).refine(
  (value) => /^[A-Za-z0-9_.-]+$/u.test(value),
  "远端文件操作标识无效。",
);
const nullableSafeInteger = z.number().int().safe().nullable();
const nullableSize = z.number().int().nonnegative().safe().nullable();

export const remoteFileKindSchema = z.enum(["directory", "file", "symlink", "other"]);

export const remoteFileEntrySchema = z.object({
  name: safeText(1_024).refine((value) => !value.includes("/"), "远端文件名无效。"),
  path: safeText(4_096),
  kind: remoteFileKindSchema,
  size: nullableSize,
  modifiedAtEpochSeconds: nullableSafeInteger,
  permissions: safeText(64).nullable(),
}).strict().superRefine((entry, context) => {
  if (entry.kind === "directory" && entry.size != null) {
    context.addIssue({ code: "custom", message: "远端目录不能携带文件大小。" });
  }
});

export const remoteDirectoryListingSchema = z.object({
  path: safeText(4_096),
  parentPath: safeText(4_096).nullable(),
  entries: z.array(remoteFileEntrySchema).max(1_000),
  truncated: z.boolean(),
}).strict();

export const remoteDirectoryResultSchema = z.discriminatedUnion("status", [
  z.object({ status: z.literal("completed"), listing: remoteDirectoryListingSchema }).strict(),
  z.object({ status: z.literal("verification_required"), prompt: safeText(64 * 1_024) }).strict(),
]);

export const remoteFileMutationResultSchema = z.object({
  committed: z.literal(true),
  refreshedListing: remoteDirectoryListingSchema.nullable(),
  message: safeText(64 * 1_024).nullable(),
}).strict();

export const remoteFileMutationResponseSchema = z.discriminatedUnion("status", [
  z.object({ status: z.literal("completed"), result: remoteFileMutationResultSchema }).strict(),
  z.object({ status: z.literal("verification_required"), prompt: safeText(64 * 1_024) }).strict(),
]);

export const remoteDownloadResponseSchema = z.discriminatedUnion("status", [
  z.object({
    status: z.literal("completed"),
    fileName: safeText(1_024).refine((value) => !value.includes("/") && !value.includes("\\")),
    size: z.number().int().nonnegative().safe(),
  }).strict(),
  z.object({ status: z.literal("verification_required"), prompt: safeText(64 * 1_024) }).strict(),
]);

export const remoteLocalFileSelectionSchema = z.object({
  selectionId: safeText(128).refine(
    (value) => value.startsWith("remote-file-selection-"),
    "本机文件必须来自原生选择器。",
  ),
  fileName: safeText(1_024).refine(
    (value) => !value.includes("/") && !value.includes("\\"),
    "本机文件名不能包含路径。",
  ),
  size: nullableSize,
}).strict();

export const remoteFileOperationInputSchema = z.object({
  operationId: identifier,
  connectionId: safeText(1_000),
  verificationCode: z.string().min(1).max(1_024).nullable(),
}).strict();

export type RemoteFileEntry = z.infer<typeof remoteFileEntrySchema>;
export type RemoteDirectoryListing = z.infer<typeof remoteDirectoryListingSchema>;
export type RemoteDirectoryResult = z.infer<typeof remoteDirectoryResultSchema>;
export type RemoteFileMutationResponse = z.infer<typeof remoteFileMutationResponseSchema>;
export type RemoteDownloadResponse = z.infer<typeof remoteDownloadResponseSchema>;
export type RemoteLocalFileSelection = z.infer<typeof remoteLocalFileSelectionSchema>;
export type RemoteFileOperationInput = z.infer<typeof remoteFileOperationInputSchema>;

export function visibleRemoteEntries(
  entries: RemoteFileEntry[],
  searchText: string,
  showsHiddenFiles: boolean,
) {
  const query = searchText.trim().toLocaleLowerCase();
  return entries.filter((entry) => (
    (showsHiddenFiles || !entry.name.startsWith("."))
    && (!query || entry.name.toLocaleLowerCase().includes(query))
  ));
}

export function formatRemoteFileSize(size: number | null) {
  if (size == null) return "—";
  if (size < 1_024) return `${size} B`;
  if (size < 1_024 ** 2) return `${(size / 1_024).toFixed(1)} KB`;
  if (size < 1_024 ** 3) return `${(size / 1_024 ** 2).toFixed(1)} MB`;
  return `${(size / 1_024 ** 3).toFixed(1)} GB`;
}
