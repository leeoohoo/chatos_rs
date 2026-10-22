import { z } from "zod";

export const petActivityKindSchema = z.enum([
  "working",
  "reviewing",
  "waiting_for_user",
  "succeeded",
  "failed",
  "cancelled",
]);

export const petAnimationStateSchema = z.enum([
  "idle",
  "running",
  "review",
  "waiting",
  "succeeded",
  "failed",
]);

export const petPresentationSchema = z.object({
  language: z.enum(["zh-CN", "en-US"]),
  animationState: petAnimationStateSchema,
  primaryActivity: z.object({
    source: z.enum(["chat", "task_execution"]),
    kind: petActivityKindSchema,
    detailCode: z.enum(["waiting_user", "waiting_tool", "retry_scheduled", "needs_review"]).nullable(),
    updatedAtUnixMs: z.number().int().nonnegative(),
  }).strict().nullable(),
  activeWorkCount: z.number().int().nonnegative(),
  attentionCount: z.number().int().nonnegative(),
}).strict();

export type PetPresentation = z.infer<typeof petPresentationSchema>;
export type PetActivityKind = z.infer<typeof petActivityKindSchema>;
export type PetAnimationState = z.infer<typeof petAnimationStateSchema>;

export const idlePetPresentation: PetPresentation = {
  language: "zh-CN",
  animationState: "idle",
  primaryActivity: null,
  activeWorkCount: 0,
  attentionCount: 0,
};
