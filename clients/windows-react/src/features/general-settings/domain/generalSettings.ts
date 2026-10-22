import { z } from "zod";

export const chatOSLanguageSchema = z.enum(["zh-CN", "en-US"]);

export const generalSettingsSchema = z.object({
  interfaceLanguage: chatOSLanguageSchema,
  contextLanguage: chatOSLanguageSchema,
  interfaceFontSize: z.number().int().min(12).max(18),
  preventsIdleSystemSleep: z.boolean(),
  revision: z.number().int().positive(),
}).strict();

export type ChatOSLanguage = z.infer<typeof chatOSLanguageSchema>;
export type GeneralSettings = z.infer<typeof generalSettingsSchema>;

export function sameGeneralSettings(left: GeneralSettings, right: GeneralSettings) {
  return left.interfaceLanguage === right.interfaceLanguage
    && left.contextLanguage === right.contextLanguage
    && left.interfaceFontSize === right.interfaceFontSize
    && left.preventsIdleSystemSleep === right.preventsIdleSystemSleep;
}
