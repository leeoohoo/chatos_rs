import { z } from "zod";
import {
  mediaStudioHistoryItemSchema,
  mediaStudioVideoHistoryItemSchema,
} from "./mediaStudio";

const identifier = z.string().trim().min(1).max(1_024);
const uuid = z.string().uuid();
const boundedText = z.string().max(20_000);

export const mediaWorkflowNodeKindSchema = z.enum(["text", "image", "video"]);
export const mediaWorkflowImageSizeSchema = z.enum(["auto", "1024x1024", "1536x1024", "1024x1536"]);
export const mediaWorkflowVideoSizeSchema = z.enum(["768P", "2K"]);
export const mediaWorkflowVideoRatioSchema = z.enum(["16:9", "9:16", "1:1", "4:3", "3:4", "21:9"]);

export const mediaWorkflowImageSizes = mediaWorkflowImageSizeSchema.options;
export const mediaWorkflowVideoSizes = mediaWorkflowVideoSizeSchema.options;
export const mediaWorkflowVideoRatios = mediaWorkflowVideoRatioSchema.options;
export const mediaWorkflowVideoSeconds = Array.from({ length: 12 }, (_, index) => index + 4);

export const mediaWorkflowImageReferenceSchema = z.object({
  id: uuid,
  name: z.string().min(1).max(255),
  mimeType: z.string().regex(/^image\//u),
  filename: z.string().min(1).max(255).refine((value) => value === value.split(/[\\/]/u).pop() && !value.startsWith(".")),
  byteCount: z.number().int().positive().max(20 * 1_024 * 1_024),
}).strict();

export const mediaWorkflowNodeSchema = z.object({
  id: uuid,
  kind: mediaWorkflowNodeKindSchema,
  modelConfigId: identifier,
  prompt: boundedText,
  referenceImages: z.array(mediaWorkflowImageReferenceSchema).max(8),
  imageSize: mediaWorkflowImageSizeSchema,
  videoSize: mediaWorkflowVideoSizeSchema,
  videoRatio: mediaWorkflowVideoRatioSchema,
  videoSeconds: z.number().int().min(4).max(15),
}).strict();

export const mediaWorkflowSchema = z.object({
  id: uuid,
  name: z.string().trim().min(1).max(128),
  inputPrompt: boundedText,
  inputImages: z.array(mediaWorkflowImageReferenceSchema).max(8),
  nodes: z.array(mediaWorkflowNodeSchema).min(1).max(8),
  createdAtUnixMs: z.number().int().nonnegative().safe(),
  updatedAtUnixMs: z.number().int().nonnegative().safe(),
}).strict().superRefine((workflow, context) => {
  if (new Set(workflow.nodes.map((node) => node.id)).size !== workflow.nodes.length) {
    context.addIssue({ code: "custom", message: "工作流包含重复节点。" });
  }
  const videos = workflow.nodes
    .map((node, index) => node.kind === "video" ? index : -1)
    .filter((index) => index >= 0);
  if (videos.length > 1 || (videos.length === 1 && videos[0] !== workflow.nodes.length - 1)) {
    context.addIssue({ code: "custom", message: "视频节点只能出现一次并且必须位于流程末尾。" });
  }
  workflow.nodes.forEach((node, index) => {
    if (node.prompt.trim().length === 0) {
      context.addIssue({ code: "custom", path: ["nodes", index, "prompt"], message: "节点提示词不能为空。" });
    }
    if (node.kind === "video" && node.referenceImages.length > 1) {
      context.addIssue({ code: "custom", path: ["nodes", index, "referenceImages"], message: "视频节点最多一张参考图。" });
    }
  });
});

export const mediaWorkflowListSchema = z.array(mediaWorkflowSchema).max(1_000);

export const mediaWorkflowNodeRunResultSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("text"), text: boundedText }).strict(),
  z.object({ type: z.literal("images"), item: mediaStudioHistoryItemSchema }).strict(),
  z.object({ type: z.literal("video"), item: mediaStudioVideoHistoryItemSchema }).strict(),
]);

export const mediaWorkflowCompletedNodeSchema = z.object({
  nodeId: uuid,
  result: mediaWorkflowNodeRunResultSchema,
}).strict();

const mediaWorkflowRunEventBaseSchema = z.object({
  ownerUserId: identifier,
  workflowId: uuid,
  clientRequestId: identifier,
  nodeId: uuid,
});

export const mediaWorkflowRunEventSchema = z.discriminatedUnion("phase", [
  mediaWorkflowRunEventBaseSchema.extend({
    phase: z.literal("running"),
    progress: boundedText.nullable(),
    result: z.null(),
    errorMessage: z.null(),
  }).strict(),
  mediaWorkflowRunEventBaseSchema.extend({
    phase: z.literal("completed"),
    progress: z.null(),
    result: mediaWorkflowNodeRunResultSchema,
    errorMessage: z.null(),
  }).strict(),
  mediaWorkflowRunEventBaseSchema.extend({
    phase: z.literal("failed"),
    progress: z.null(),
    result: z.null(),
    errorMessage: boundedText.min(1),
  }).strict(),
]);

export const mediaWorkflowRunResultSchema = z.object({
  workflowId: uuid,
  clientRequestId: identifier,
  status: z.literal("completed"),
  finalText: boundedText,
  imageRecordIds: z.array(uuid).max(8),
  videoRecordIds: z.array(uuid).max(1),
  nodeResults: z.array(mediaWorkflowCompletedNodeSchema).max(8).superRefine((results, context) => {
    if (new Set(results.map((result) => result.nodeId)).size !== results.length) {
      context.addIssue({ code: "custom", message: "工作流运行结果包含重复节点。" });
    }
  }),
  errorMessage: boundedText.nullable(),
}).strict();

export type MediaWorkflowRunResult = z.infer<typeof mediaWorkflowRunResultSchema>;
export type MediaWorkflowNodeRunResult = z.infer<typeof mediaWorkflowNodeRunResultSchema>;
export type MediaWorkflowRunEvent = z.infer<typeof mediaWorkflowRunEventSchema>;

export type MediaWorkflowNodeKind = z.infer<typeof mediaWorkflowNodeKindSchema>;
export type MediaWorkflowImageReference = z.infer<typeof mediaWorkflowImageReferenceSchema>;
export type MediaWorkflowNode = z.infer<typeof mediaWorkflowNodeSchema>;
export type MediaWorkflow = z.infer<typeof mediaWorkflowSchema>;

export function newMediaWorkflow(
  modelConfigId = "",
  kind: MediaWorkflowNodeKind = "image",
): MediaWorkflow {
  const now = Date.now();
  return {
    id: crypto.randomUUID(),
    name: "工作流 1",
    inputPrompt: "",
    inputImages: [],
    nodes: [newMediaWorkflowNode(kind, modelConfigId)],
    createdAtUnixMs: now,
    updatedAtUnixMs: now,
  };
}

export function newMediaWorkflowNode(kind: MediaWorkflowNodeKind, modelConfigId = ""): MediaWorkflowNode {
  return {
    id: crypto.randomUUID(),
    kind,
    modelConfigId,
    prompt: kind === "text"
      ? "优化当前工作文本"
      : kind === "image"
        ? "根据当前文本生成图片"
        : "根据当前文本生成视频",
    referenceImages: [],
    imageSize: "1024x1024",
    videoSize: "768P",
    videoRatio: "16:9",
    videoSeconds: 4,
  };
}

export function mediaWorkflowNodeLabel(kind: MediaWorkflowNodeKind) {
  return kind === "text" ? "文本" : kind === "image" ? "图片" : "视频";
}
