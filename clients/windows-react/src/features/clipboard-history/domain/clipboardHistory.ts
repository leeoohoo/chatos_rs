import { z } from "zod";

const clipboardPreviewUrlSchema = z.string().refine((value) => {
  try {
    const url = new URL(value);
    return (url.protocol === "chatos-clipboard:" && url.hostname === "localhost")
      || (url.protocol === "http:" && url.hostname === "chatos-clipboard.localhost");
  } catch {
    return false;
  }
}, "剪贴板预览地址不受信任。");

export const clipboardEntrySchema = z.object({
  id: z.uuid(),
  kind: z.enum(["text", "url", "files", "image"]),
  createdAtUnixMs: z.number().int(),
  updatedAtUnixMs: z.number().int(),
  preview: z.string().max(360).nullable(),
  byteCount: z.number().int().positive().max(25 * 1024 * 1024),
  isPinned: z.boolean(),
  previewUrl: clipboardPreviewUrlSchema.nullable(),
}).strict().superRefine((entry, context) => {
  if ((entry.kind === "image") !== (entry.previewUrl != null)) {
    context.addIssue({ code: "custom", message: "剪贴板图片预览与内容类型不匹配。" });
  }
});

export const clipboardHistorySchema = z.object({
  language: z.enum(["zh-CN", "en-US"]),
  entries: z.array(clipboardEntrySchema).max(500),
}).strict();

export type ClipboardEntry = z.infer<typeof clipboardEntrySchema>;
export type ClipboardHistory = z.infer<typeof clipboardHistorySchema>;
