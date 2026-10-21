import { z } from "zod";

const safeText = (maximum: number) => z.string().max(maximum).refine(
  (value) => !/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/u.test(value),
  "剧情字段包含不安全的控制字符。",
);
const identifier = safeText(1_024).min(1).refine((value) => value.trim() === value);
const uuid = z.string().uuid().refine((value) => value === value.toLowerCase());
const fingerprint = z.string().regex(/^[0-9a-f]{64}$/u);
const unixMilliseconds = z.number().int().nonnegative().safe();
const previewUrl = z.string().max(2_048).refine((value) => {
  try {
    const parsed = new URL(value);
    return (parsed.protocol === "http:" && parsed.hostname === "chatos-story.localhost")
      || (parsed.protocol === "chatos-story:" && parsed.hostname === "localhost");
  } catch {
    return false;
  }
});

export const storyImageSchema = z.object({
  id: identifier,
  mimeType: z.enum(["image/png", "image/jpeg", "image/webp"]),
  contentToken: z.string().regex(/^[0-9a-f]{32}$/u).nullable(),
  previewUrl: previewUrl.nullable(),
}).strict();

export const storyImageCollectionSchema = z.object({
  images: z.array(storyImageSchema).max(1_000),
  confirmedImageId: identifier.nullable(),
  generationAttemptId: uuid.nullable(),
}).strict();

const characterProfileSchema = z.object({
  isProtagonist: z.boolean(),
  roleInStory: safeText(600),
  appearance: safeText(600),
  personality: safeText(600),
  motivation: safeText(600),
  relationships: safeText(600),
  costume: safeText(600),
  consistencyNotes: safeText(600),
}).strict();

const sceneProfileSchema = z.object({
  roleInStory: safeText(600),
  setting: safeText(600),
  spatialLayout: safeText(600),
  lightingAndPalette: safeText(600),
  keyElements: safeText(600),
  atmosphere: safeText(600),
  consistencyNotes: safeText(600),
}).strict();

const storyCharacterSchema = z.object({
  id: identifier,
  name: safeText(1_024).min(1),
  profile: characterProfileSchema,
  imagePrompt: safeText(20_000),
  media: storyImageCollectionSchema,
}).strict();

const storySceneSchema = z.object({
  id: identifier,
  name: safeText(1_024).min(1),
  profile: sceneProfileSchema,
  imagePrompt: safeText(20_000),
  media: storyImageCollectionSchema,
}).strict();

const storyPropSchema = z.object({
  id: identifier,
  name: safeText(1_024).min(1),
  description: safeText(4_000),
  imagePrompt: safeText(20_000),
  media: storyImageCollectionSchema,
}).strict();

const storySegmentDetailSchema = z.object({
  firstFramePrompt: safeText(3_000).min(1),
  lastFramePrompt: safeText(3_000).nullable(),
  shots: z.array(z.object({
    start: z.number().int().min(0).max(15),
    end: z.number().int().min(1).max(15),
    prompt: safeText(2_000).min(1),
  }).strict()).min(1).max(8),
  continuityIn: safeText(1_000),
  continuityOut: safeText(1_000),
  audio: safeText(1_000),
  constraints: safeText(2_000),
}).strict();

const storyVideoAttemptSchema = z.object({
  id: uuid,
  modelConfigId: identifier,
  prompt: safeText(7_000).min(1),
  size: z.enum(["768P", "2K"]),
  ratio: safeText(32).min(1),
  seconds: z.number().int().min(2).max(15),
  status: safeText(128).min(1),
  createdAtUnixMs: unixMilliseconds,
  resumable: z.boolean(),
}).strict();

const storyVideoSchema = z.object({
  modelName: safeText(1_024).min(1),
  contentToken: z.string().regex(/^[0-9a-f]{32}$/u).nullable(),
  previewUrl: previewUrl.nullable(),
}).strict();

export const storySegmentSchema = z.object({
  id: identifier,
  title: safeText(120).min(1),
  synopsis: safeText(2_000),
  kind: z.enum(["story", "transition"]),
  seconds: z.number().int().min(2).max(15),
  sourceRange: z.object({
    start: z.number().int().nonnegative(),
    end: z.number().int().nonnegative(),
  }).strict(),
  characterIds: z.array(identifier).max(8),
  sceneIds: z.array(identifier).max(8),
  propIds: z.array(identifier).max(8),
  detail: storySegmentDetailSchema.nullable(),
  firstFrames: storyImageCollectionSchema,
  userSelectedFirstFrameId: identifier.nullable(),
  inheritedFirstFrameSourceSegmentId: identifier.nullable(),
  lastFrames: storyImageCollectionSchema,
  actualVideoLastFrameId: identifier.nullable(),
  videoGuidanceMode: z.enum([
    "first_frame",
    "first_and_last_frames",
    "previous_video",
    "source_video",
  ]),
  attempt: storyVideoAttemptSchema.nullable(),
  previousAttempts: z.array(storyVideoAttemptSchema).max(100),
  video: storyVideoSchema.nullable(),
  archivedVideos: z.array(storyVideoSchema).max(100),
  error: safeText(2_000).nullable(),
}).strict();

const storyRelationSchema = z.object({
  id: identifier,
  segmentId: identifier,
  characterId: identifier,
  sceneId: identifier,
  action: safeText(400),
  position: safeText(400),
  startSecond: z.number().int().min(0).max(15),
  endSecond: z.number().int().min(1).max(15),
}).strict();

export const storyProjectSchema = z.object({
  id: uuid,
  baseFingerprint: fingerprint.optional(),
  version: z.literal(2),
  title: safeText(120).min(1),
  description: safeText(4_000),
  models: z.object({
    textModelId: identifier,
    imageModelId: identifier,
    videoModelId: identifier,
    supportedVideoDurations: z.array(z.number().int().min(2).max(30)).nullable(),
  }).strict(),
  source: safeText(80_000),
  style: safeText(2_000),
  ratio: safeText(32).min(1),
  summary: safeText(16_000),
  characters: z.array(storyCharacterSchema).max(100),
  scenes: z.array(storySceneSchema).max(100),
  props: z.array(storyPropSchema).max(100),
  segments: z.array(storySegmentSchema).max(200),
  relations: z.array(storyRelationSchema).max(1_600),
  createdAtUnixMs: unixMilliseconds,
  updatedAtUnixMs: unixMilliseconds,
}).strict();

export const storyProjectCursorSchema = z.object({
  modifiedAtUnixMs: unixMilliseconds,
  recordId: uuid,
}).strict();

export const storyProjectPageSchema = z.object({
  items: z.array(storyProjectSchema).max(200),
  nextCursor: storyProjectCursorSchema.nullable(),
  unreadableCount: z.number().int().nonnegative().safe(),
}).strict();

export const storyVideoGenerationOutcomeSchema = z.object({
  project: storyProjectSchema,
  status: safeText(128).min(1),
  percent: z.number().finite().min(0).max(100).nullable(),
  terminal: z.boolean(),
}).strict();

export const storyMediaBatchKindSchema = z.enum([
  "assets",
  "frames",
  "lastFrames",
  "videos",
  "pipeline",
]);

export const storyMediaBatchPreparationSchema = z.object({
  previewId: uuid,
  projectId: uuid,
  projectFingerprint: fingerprint,
  kind: storyMediaBatchKindSchema,
  eligibleTargetIds: z.array(identifier).max(500),
  requestedTargetIds: z.array(identifier).min(1).max(500),
  steps: z.array(z.object({
    id: identifier,
    kind: storyMediaBatchKindSchema.exclude(["pipeline"]),
    targetId: identifier,
  }).strict()).min(1).max(500),
  executionMode: z.enum(["persistentBatch", "directVideos"]),
}).strict().superRefine((preview, context) => {
  if (new Set(preview.eligibleTargetIds).size !== preview.eligibleTargetIds.length
    || new Set(preview.requestedTargetIds).size !== preview.requestedTargetIds.length
    || new Set(preview.steps.map((step) => step.id)).size !== preview.steps.length
    || preview.requestedTargetIds.some((id) => !preview.eligibleTargetIds.includes(id))
    || (preview.kind === "videos") !== (preview.executionMode === "directVideos")) {
    context.addIssue({ code: "custom", message: "批量制作预览身份不一致。" });
  }
});

export const storyMediaBatchPreviewResultSchema = z.object({
  eligibleTargetIds: z.array(identifier).max(500),
  preparation: storyMediaBatchPreparationSchema.nullable(),
  validationError: safeText(2_000).nullable(),
}).strict().superRefine((result, context) => {
  if (new Set(result.eligibleTargetIds).size !== result.eligibleTargetIds.length
    || (result.preparation != null && result.validationError != null)
    || (result.preparation == null
      && result.eligibleTargetIds.length > 0
      && result.validationError == null)) {
    context.addIssue({ code: "custom", message: "批量制作预览结果不一致。" });
  }
  if (result.preparation
    && result.preparation.eligibleTargetIds.some((id, index) => id !== result.eligibleTargetIds[index])) {
    context.addIssue({ code: "custom", message: "批量制作候选目标不一致。" });
  }
});

export const storyPlanningRunSchema = z.object({
  id: uuid,
  ownerUserId: safeText(1_024).min(1),
  projectId: uuid,
  stage: z.enum(["outline", "refine"]),
  targetIds: z.array(identifier).max(200),
  baseProjectFingerprint: fingerprint,
  draft: storyProjectSchema,
  events: z.array(z.object({
    id: uuid,
    dateUnixMs: unixMilliseconds,
    kind: safeText(1_024).min(1),
    detail: safeText(1_000),
    modelCalls: z.number().int().nonnegative().safe(),
  }).strip()).max(100_000),
  status: z.enum(["ready", "running", "paused", "completed", "failed", "needs_review", "limit_reached"]),
  stopReason: safeText(2_000).nullable(),
  modelCalls: z.number().int().nonnegative().safe(),
  usesCloudMemory: z.boolean(),
  applied: z.boolean(),
  abandonedAtUnixMs: unixMilliseconds.nullable(),
  updatedAtUnixMs: unixMilliseconds,
}).strip();

export const storyPlanningHistoryPageSchema = z.object({
  runs: z.array(storyPlanningRunSchema).max(200),
  nextCursor: storyProjectCursorSchema.nullable(),
  unreadableCount: z.number().int().nonnegative().safe(),
  recoveredProject: storyProjectSchema.nullable(),
}).strict();

export const storyPlanningOutcomeSchema = z.object({
  run: storyPlanningRunSchema,
  project: storyProjectSchema.nullable(),
  stopReason: safeText(2_000).nullable(),
}).strip();

export const storyPlanningEventSchema = z.object({
  presentationId: uuid,
  run: storyPlanningRunSchema,
}).strip();

export type StoryProject = z.infer<typeof storyProjectSchema>;
export type StoryProjectCursor = z.infer<typeof storyProjectCursorSchema>;
export type StoryProjectPage = z.infer<typeof storyProjectPageSchema>;
export type StorySegment = z.infer<typeof storySegmentSchema>;
export type StoryVideoGenerationOutcome = z.infer<typeof storyVideoGenerationOutcomeSchema>;
export type StoryMediaBatchKind = z.infer<typeof storyMediaBatchKindSchema>;
export type StoryMediaBatchPreparation = z.infer<typeof storyMediaBatchPreparationSchema>;
export type StoryMediaBatchPreviewResult = z.infer<typeof storyMediaBatchPreviewResultSchema>;
export type StoryPlanningRun = z.infer<typeof storyPlanningRunSchema>;
export type StoryPlanningHistoryPage = z.infer<typeof storyPlanningHistoryPageSchema>;
export type StoryPlanningEvent = z.infer<typeof storyPlanningEventSchema>;

export function storyConfirmedImage(collection: z.infer<typeof storyImageCollectionSchema>) {
  return collection.images.find((image) => image.id === collection.confirmedImageId) ?? null;
}

export function isStorySegmentReady(segment: StorySegment) {
  return segment.detail != null
    && storyConfirmedImage(segment.firstFrames) != null
    && segment.attempt == null
    && segment.video == null;
}
