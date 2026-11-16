import { z } from "zod";

const identifier = z.string().trim().min(1).max(1_024);
const uuid = z.string().uuid();
const boundedText = z.string().max(20_000);

export const mediaWorkflowNodeKindSchema = z.enum(["text", "image", "video"]);

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
  imageSize: z.string().min(1).max(128),
  videoSize: z.string().min(1).max(128),
  videoRatio: z.string().min(1).max(32),
  videoSeconds: z.number().int().min(1).max(120),
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

export type MediaWorkflowNodeKind = z.infer<typeof mediaWorkflowNodeKindSchema>;
export type MediaWorkflowImageReference = z.infer<typeof mediaWorkflowImageReferenceSchema>;
export type MediaWorkflowNode = z.infer<typeof mediaWorkflowNodeSchema>;
export type MediaWorkflow = z.infer<typeof mediaWorkflowSchema>;

export function newMediaWorkflow(modelConfigId = ""): MediaWorkflow {
  const now = Date.now();
  return {
    id: crypto.randomUUID(),
    name: "工作流 1",
    inputPrompt: "",
    inputImages: [],
    nodes: [newMediaWorkflowNode("image", modelConfigId)],
    createdAtUnixMs: now,
    updatedAtUnixMs: now,
  };
}

export function newMediaWorkflowNode(kind: MediaWorkflowNodeKind, modelConfigId = ""): MediaWorkflowNode {
  return {
    id: crypto.randomUUID(),
    kind,
    modelConfigId,
    prompt: "",
    referenceImages: [],
    imageSize: "1024x1024",
    videoSize: "1280x720",
    videoRatio: "16:9",
    videoSeconds: 4,
  };
}

export function mediaWorkflowNodeLabel(kind: MediaWorkflowNodeKind) {
  return kind === "text" ? "文本" : kind === "image" ? "图片" : "视频";
}
