import { z } from "zod";

export const quickSearchLanguageSchema = z.enum(["zh-CN", "en-US"]);

export const quickSearchBootstrapSchema = z.object({
  presentationId: z.string().uuid(),
  language: quickSearchLanguageSchema,
}).strict();

export const quickSearchResultSchema = z.object({
  id: z.string().uuid(),
  kind: z.enum(["chatOs", "application", "file", "action"]),
  title: z.string().min(1).max(512),
  subtitle: z.string().max(512).nullable().optional(),
  icon: z.enum([
    "project",
    "contact",
    "application",
    "file",
    "folder",
    "screenshot",
    "screenRecording",
    "clipboard",
    "settings",
    "permissions",
  ]),
}).strict();

export const quickSearchSnapshotSchema = z.object({
  presentationId: z.string().uuid(),
  requestId: z.number().int().positive(),
  results: z.array(quickSearchResultSchema).max(50),
  isFileSearchTimedOut: z.boolean(),
  diagnostic: z.string().nullable().optional(),
}).strict();

export type QuickSearchBootstrap = z.infer<typeof quickSearchBootstrapSchema>;
export type QuickSearchResult = z.infer<typeof quickSearchResultSchema>;
export type QuickSearchSnapshot = z.infer<typeof quickSearchSnapshotSchema>;

export function quickSearchAllowsFileLookup(query: string): boolean {
  const trimmed = query.trim();
  if (trimmed.startsWith(">") || trimmed.startsWith("@")) return false;
  const effective = trimmed.startsWith("/") ? trimmed.slice(1).trim() : trimmed;
  return Array.from(effective).length >= 2;
}
