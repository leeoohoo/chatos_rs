import { z } from "zod";

export const petTranslationTargetSchema = z.enum(["automatic", "simplifiedChinese", "english"]);
export const petTranslationOutputStyleSchema = z.enum(["bilingual", "translationOnly"]);

export const petTranslationRecordSchema = z.object({
  id: z.uuid(),
  sourceText: z.string().max(20_000),
  translatedText: z.string().min(1).max(40_000),
  attachments: z.array(z.object({
    name: z.string().min(1).max(255),
    mimeType: z.string().min(1).max(255),
    isImage: z.literal(true),
  }).strict()).max(10).default([]),
  target: petTranslationTargetSchema,
  outputStyle: petTranslationOutputStyleSchema,
  createdAtUnixMs: z.number().int().nonnegative(),
}).strict();

export const petTranslationHistorySchema = z.object({
  records: z.array(petTranslationRecordSchema).max(100),
}).strict();

export type PetTranslationTarget = z.infer<typeof petTranslationTargetSchema>;
export type PetTranslationOutputStyle = z.infer<typeof petTranslationOutputStyleSchema>;
export type PetTranslationRecord = z.infer<typeof petTranslationRecordSchema>;
export type PetTranslationHistory = z.infer<typeof petTranslationHistorySchema>;

export interface PetTranslationRequest {
  ownerUserId: string;
  sourceText: string;
  target: PetTranslationTarget;
  outputStyle: PetTranslationOutputStyle;
  attachmentIds: string[];
}
