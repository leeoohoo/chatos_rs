import { z } from "zod";

const MAXIMUM_IMAGE_BYTES = 20 * 1_024 * 1_024;
const MAXIMUM_AUDIO_BYTES = 20 * 1_024 * 1_024;
const MAXIMUM_BASE64_CHARACTERS = Math.ceil(MAXIMUM_IMAGE_BYTES / 3) * 4;

const safeText = (maximum: number) => z.string().max(maximum).refine(
  (value) => !/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/u.test(value),
  "媒体字段包含不安全的控制字符。",
);

const identifierSchema = safeText(1_024).min(1).refine(
  (value) => value.trim() === value,
  "媒体标识不能包含首尾空白。",
);

export const mediaStudioUuidSchema = z.string().uuid().refine(
  (value) => value === value.toLowerCase(),
  "媒体标识必须使用规范 UUID。",
);

const contentTokenSchema = z.string().regex(/^[0-9a-f]{32}$/u);
export const mediaStudioReferenceIdSchema = z.string().regex(/^reference_[0-9a-f]{32}$/u);
const imageMimeTypeSchema = z.enum(["image/png", "image/jpeg", "image/webp", "image/gif"]);
const referenceMimeTypeSchema = z.enum(["image/png", "image/jpeg", "image/webp"]);
const videoMimeTypeSchema = z.enum(["video/mp4", "video/quicktime"]);
const audioMimeTypeSchema = z.enum(["audio/mpeg", "audio/wav", "audio/mp4", "audio/aac"]);
const unixMillisecondsSchema = z.number().int().nonnegative().safe();

export const mediaStudioPreviewUrlSchema = z.string().max(2_048).refine(
  isRestrictedMediaPreviewUrl,
  "图片预览地址不是受控的 chatos-media 地址。",
);

export const mediaStudioModelSchema = z.object({
  id: identifierSchema,
  name: safeText(1_024).min(1),
  provider: safeText(1_024).min(1),
  modelName: safeText(1_024).min(1),
  enabled: z.boolean(),
  taskEnabled: z.boolean(),
  hasApiKey: z.boolean(),
}).strict();

export const mediaStudioModelListSchema = z.array(mediaStudioModelSchema).max(1_000);

export const mediaStudioHistoryAssetSchema = z.object({
  id: identifierSchema,
  mimeType: imageMimeTypeSchema,
  contentToken: contentTokenSchema,
  revisedPrompt: safeText(100_000).min(1).nullable(),
  previewUrl: mediaStudioPreviewUrlSchema.nullable(),
}).strict();

export const mediaStudioHistoryItemSchema = z.object({
  id: mediaStudioUuidSchema,
  prompt: safeText(100_000),
  modelName: safeText(1_024).min(1),
  createdAtUnixMs: unixMillisecondsSchema,
  images: z.array(mediaStudioHistoryAssetSchema).min(1).max(10),
}).strict().superRefine((item, context) => {
  if (new Set(item.images.map((image) => image.id)).size !== item.images.length) {
    context.addIssue({ code: "custom", message: "图片历史包含重复资源。" });
  }
  for (const image of item.images) {
    if (image.previewUrl != null && !isHistoryPreviewForAsset(
      image.previewUrl,
      item.id,
      image.contentToken,
    )) {
      context.addIssue({ code: "custom", message: "图片历史预览地址与资源不匹配。" });
      break;
    }
  }
});

export const mediaStudioHistoryCursorSchema = z.object({
  modifiedAtUnixMs: unixMillisecondsSchema.positive(),
  recordId: mediaStudioUuidSchema,
}).strict();

export const mediaStudioHistoryPageSchema = z.object({
  items: z.array(mediaStudioHistoryItemSchema).max(200),
  nextCursor: mediaStudioHistoryCursorSchema.nullable(),
  unreadableCount: z.number().int().nonnegative().safe().max(100_000),
}).strict().superRefine((page, context) => {
  if (new Set(page.items.map((item) => item.id)).size !== page.items.length) {
    context.addIssue({ code: "custom", message: "图片历史页包含重复记录。" });
  }
});

export const mediaStudioReferenceSchema = z.object({
  id: mediaStudioReferenceIdSchema,
  name: safeText(1_024).min(1),
  mimeType: referenceMimeTypeSchema,
  byteSize: z.number().int().positive().safe().max(MAXIMUM_IMAGE_BYTES),
  previewUrl: mediaStudioPreviewUrlSchema.nullable(),
}).strict();

export const mediaStudioReferenceImportResultSchema = z.object({
  references: z.array(mediaStudioReferenceSchema).max(8),
  errors: z.array(safeText(16 * 1_024).min(1)).max(32),
}).strict().superRefine((result, context) => {
  if (new Set(result.references.map((reference) => reference.id)).size !== result.references.length) {
    context.addIssue({ code: "custom", message: "参考图导入结果包含重复资源。" });
  }
});

const base64ImageSchema = z.string()
  .min(1)
  .max(MAXIMUM_BASE64_CHARACTERS)
  .regex(/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/u);

const remoteImageUrlSchema = z.string().max(16 * 1_024).url().refine((value) => {
  try {
    const url = new URL(value);
    return url.protocol === "https:"
      && url.hostname.length > 0
      && url.username === ""
      && url.password === "";
  } catch {
    return false;
  }
}, "生成图片地址必须是不含凭据的 HTTPS URL。");

export const mediaStudioGeneratedAssetSchema = z.object({
  id: identifierSchema,
  mimeType: imageMimeTypeSchema,
  base64Data: base64ImageSchema.nullable(),
  url: remoteImageUrlSchema.nullable(),
  revisedPrompt: safeText(100_000).min(1).nullable(),
}).strict().refine(
  (asset) => (asset.base64Data == null) !== (asset.url == null),
  "生成图片必须且只能包含一种内容来源。",
);

export const mediaStudioGenerationResultSchema = z.object({
  id: identifierSchema,
  modelConfigId: identifierSchema,
  modelName: safeText(1_024).min(1),
  createdAtUnixMs: unixMillisecondsSchema,
  images: z.array(mediaStudioGeneratedAssetSchema).min(1).max(10),
  clientRequestId: identifierSchema,
}).strict().superRefine((result, context) => {
  if (new Set(result.images.map((image) => image.id)).size !== result.images.length) {
    context.addIssue({ code: "custom", message: "生成结果包含重复图片。" });
  }
});

export const mediaStudioGenerationOutcomeSchema = z.discriminatedUnion("status", [
  z.object({
    status: z.literal("saved"),
    item: mediaStudioHistoryItemSchema,
  }).strict(),
  z.object({
    status: z.literal("generatedWithoutPersistence"),
    result: mediaStudioGenerationResultSchema,
    persistenceErrorMessage: safeText(16 * 1_024).min(1),
  }).strict(),
]);

export const mediaStudioGenerationRequestSchema = z.object({
  presentationId: mediaStudioUuidSchema,
  modelConfigId: identifierSchema,
  prompt: safeText(100_000).refine((value) => value.trim().length > 0, "请输入图片描述。"),
  size: safeText(128).min(1).nullable(),
  count: z.number().int().min(1).max(10),
  referenceImageIds: z.array(mediaStudioReferenceIdSchema).max(8),
  clientRequestId: identifierSchema,
}).strict().superRefine((request, context) => {
  if (new Set(request.referenceImageIds).size !== request.referenceImageIds.length) {
    context.addIssue({ code: "custom", message: "图片生成请求包含重复参考图。" });
  }
});

export const mediaStudioVideoHistoryItemSchema = z.object({
  id: mediaStudioUuidSchema,
  prompt: safeText(100_000),
  modelName: safeText(1_024).min(1),
  createdAtUnixMs: unixMillisecondsSchema,
  mimeType: videoMimeTypeSchema,
  contentToken: contentTokenSchema,
  previewUrl: mediaStudioPreviewUrlSchema.nullable(),
}).strict().superRefine((item, context) => {
  if (item.previewUrl != null && !isHistoryPreviewForAsset(
    item.previewUrl,
    item.id,
    item.contentToken,
  )) {
    context.addIssue({ code: "custom", message: "视频历史预览地址与资源不匹配。" });
  }
});

export const mediaStudioVideoHistoryCursorSchema = mediaStudioHistoryCursorSchema;

export const mediaStudioVideoHistoryPageSchema = z.object({
  items: z.array(mediaStudioVideoHistoryItemSchema).max(200),
  nextCursor: mediaStudioVideoHistoryCursorSchema.nullable(),
  unreadableCount: z.number().int().nonnegative().safe().max(100_000),
}).strict().superRefine((page, context) => {
  if (new Set(page.items.map((item) => item.id)).size !== page.items.length) {
    context.addIssue({ code: "custom", message: "视频历史页包含重复记录。" });
  }
});

export const mediaStudioVideoAudioDraftSchema = z.object({
  id: z.string().regex(/^video_audio_[0-9a-f]{32}$/u),
  name: safeText(1_024).min(1),
  mimeType: audioMimeTypeSchema,
  byteSize: z.number().int().positive().safe().max(MAXIMUM_AUDIO_BYTES),
}).strict();

export const mediaStudioVideoSizeSchema = z.enum(["768P", "2K"]);
export const mediaStudioVideoRatioSchema = z.enum(["16:9", "9:16", "1:1", "4:3", "3:4", "21:9"]);

export const mediaStudioVideoGenerationRequestSchema = z.object({
  presentationId: mediaStudioUuidSchema,
  modelConfigId: identifierSchema,
  prompt: safeText(7_000).refine((value) => value.trim().length > 0, "请输入视频描述。"),
  size: mediaStudioVideoSizeSchema,
  seconds: z.number().int().min(4).max(15),
  ratio: mediaStudioVideoRatioSchema,
  inputImageId: mediaStudioReferenceIdSchema.nullable(),
  referenceAudioId: z.string().regex(/^video_audio_[0-9a-f]{32}$/u).nullable(),
  clientRequestId: identifierSchema,
}).strict();

export const mediaStudioVideoJobStatusSchema = z.enum([
  "queued",
  "in_progress",
  "unknown",
  "downloading",
  "saving",
  "failed",
  "completed",
  "cancelled",
]);

export const mediaStudioVideoJobSnapshotSchema = z.object({
  jobId: mediaStudioUuidSchema,
  clientRequestId: identifierSchema,
  status: mediaStudioVideoJobStatusSchema,
  percent: z.number().finite().min(0).max(100).nullable(),
  providerJobId: identifierSchema.nullable(),
  result: mediaStudioVideoHistoryItemSchema.nullable(),
  errorMessage: safeText(16 * 1_024).min(1).nullable(),
  terminal: z.boolean(),
  revision: z.number().int().positive().safe(),
}).strict().superRefine((snapshot, context) => {
  if (snapshot.result != null && (snapshot.status !== "completed" || !snapshot.terminal)) {
    context.addIssue({ code: "custom", message: "视频结果只能出现在已完成的终态任务中。" });
  }
  if ((snapshot.status === "failed" || snapshot.status === "cancelled") && !snapshot.terminal) {
    context.addIssue({ code: "custom", message: "视频失败或取消状态必须是终态。" });
  }
  if (snapshot.status === "failed" && snapshot.errorMessage == null) {
    context.addIssue({ code: "custom", message: "视频失败状态缺少错误信息。" });
  }
});

export type MediaStudioModel = z.infer<typeof mediaStudioModelSchema>;
export type MediaStudioHistoryAsset = z.infer<typeof mediaStudioHistoryAssetSchema>;
export type MediaStudioHistoryItem = z.infer<typeof mediaStudioHistoryItemSchema>;
export type MediaStudioHistoryCursor = z.infer<typeof mediaStudioHistoryCursorSchema>;
export type MediaStudioHistoryPage = z.infer<typeof mediaStudioHistoryPageSchema>;
export type MediaStudioReference = z.infer<typeof mediaStudioReferenceSchema>;
export type MediaStudioReferenceImportResult = z.infer<typeof mediaStudioReferenceImportResultSchema>;
export type MediaStudioGeneratedAsset = z.infer<typeof mediaStudioGeneratedAssetSchema>;
export type MediaStudioGenerationResult = z.infer<typeof mediaStudioGenerationResultSchema>;
export type MediaStudioGenerationOutcome = z.infer<typeof mediaStudioGenerationOutcomeSchema>;
export type MediaStudioGenerationRequest = z.infer<typeof mediaStudioGenerationRequestSchema>;
export type MediaStudioVideoHistoryItem = z.infer<typeof mediaStudioVideoHistoryItemSchema>;
export type MediaStudioVideoHistoryCursor = z.infer<typeof mediaStudioVideoHistoryCursorSchema>;
export type MediaStudioVideoHistoryPage = z.infer<typeof mediaStudioVideoHistoryPageSchema>;
export type MediaStudioVideoAudioDraft = z.infer<typeof mediaStudioVideoAudioDraftSchema>;
export type MediaStudioVideoSize = z.infer<typeof mediaStudioVideoSizeSchema>;
export type MediaStudioVideoRatio = z.infer<typeof mediaStudioVideoRatioSchema>;
export type MediaStudioVideoGenerationRequest = z.infer<typeof mediaStudioVideoGenerationRequestSchema>;
export type MediaStudioVideoJobStatus = z.infer<typeof mediaStudioVideoJobStatusSchema>;
export type MediaStudioVideoJobSnapshot = z.infer<typeof mediaStudioVideoJobSnapshotSchema>;

export function generatedAssetPreviewUrl(asset: z.infer<typeof mediaStudioGeneratedAssetSchema>) {
  return asset.url ?? `data:${asset.mimeType};base64,${asset.base64Data ?? ""}`;
}

export function isReferencePreviewForPresentation(value: string, presentationId: string) {
  const path = mediaPreviewPath(value);
  return path?.kind === "reference" && path.presentationId === presentationId;
}

function isRestrictedMediaPreviewUrl(value: string) {
  return mediaPreviewPath(value) != null;
}

function isHistoryPreviewForAsset(value: string, recordId: string, contentToken: string) {
  const path = mediaPreviewPath(value);
  return path?.kind === "history"
    && path.recordId === recordId
    && path.contentToken === contentToken;
}

type MediaPreviewPath =
  | { kind: "history"; recordId: string; contentToken: string }
  | { kind: "reference"; presentationId: string };

function mediaPreviewPath(value: string): MediaPreviewPath | null {
  try {
    const url = new URL(value);
    const isNative = url.protocol === "chatos-media:" && url.hostname === "localhost";
    const isWindows = url.protocol === "http:"
      && url.hostname === "chatos-media.localhost"
      && url.port === "";
    if ((!isNative && !isWindows)
      || url.username !== ""
      || url.password !== ""
      || url.search !== ""
      || url.hash !== "") {
      return null;
    }
    const parts = url.pathname.split("/").filter(Boolean);
    if (parts.length === 3
      && mediaStudioUuidSchema.safeParse(parts[0]).success
      && mediaStudioUuidSchema.safeParse(parts[1]).success
      && contentTokenSchema.safeParse(parts[2]).success) {
      return { kind: "history", recordId: parts[1]!, contentToken: parts[2]! };
    }
    if (parts.length === 4
      && mediaStudioUuidSchema.safeParse(parts[0]).success
      && parts[1] === "reference"
      && mediaStudioUuidSchema.safeParse(parts[2]).success
      && mediaStudioUuidSchema.safeParse(parts[3]).success) {
      return { kind: "reference", presentationId: parts[2]! };
    }
    return null;
  } catch {
    return null;
  }
}
