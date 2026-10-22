import { z } from "zod";

const optionalText = z.string().nullable();

export const aiModelConfigSchema = z.object({
  id: z.string().min(1),
  sourceProviderId: optionalText,
  name: z.string().min(1),
  provider: z.string().min(1),
  promptVendor: z.string().min(1),
  baseUrl: z.string(),
  modelName: z.string().min(1),
  taskUsageScenario: optionalText,
  taskThinkingLevel: optionalText,
  temperature: z.number().nullable(),
  maxOutputTokens: z.number().int().positive().nullable(),
  enabled: z.boolean(),
  taskEnabled: z.boolean(),
  hasApiKey: z.boolean(),
  supportsImages: z.boolean(),
  supportsReasoning: z.boolean(),
  supportsResponses: z.boolean(),
  thinkingLevels: z.array(z.string()),
});

export const aiModelProviderSchema = z.object({
  id: z.string().min(1),
  name: z.string().min(1),
  provider: z.string().min(1),
  promptVendor: z.string().min(1),
  baseUrl: z.string(),
  hasApiKey: z.boolean(),
  enabled: z.boolean(),
  supportsImages: z.boolean(),
  supportsReasoning: z.boolean(),
  supportsResponses: z.boolean(),
  lastSyncStatus: optionalText,
  lastSyncError: optionalText,
  importedModelCount: z.number().int().nonnegative(),
});

export const aiModelDefaultsSchema = z.object({
  modelRequestMaxRetries: z.number().int().min(0).max(10),
  memorySummaryModelId: optionalText,
  memorySummaryThinkingLevel: optionalText,
  approvalModelId: optionalText,
  approvalThinkingLevel: optionalText,
});

export const aiModelSettingsSnapshotSchema = z.object({
  models: z.array(aiModelConfigSchema),
  providers: z.array(aiModelProviderSchema),
  settings: aiModelDefaultsSchema,
});

export type AiModelConfig = z.infer<typeof aiModelConfigSchema>;
export type AiModelProvider = z.infer<typeof aiModelProviderSchema>;
export type AiModelDefaults = z.infer<typeof aiModelDefaultsSchema>;
export type AiModelSettingsSnapshot = z.infer<typeof aiModelSettingsSnapshotSchema>;

export interface AiModelProviderDraft {
  providerId: string | null;
  name: string;
  promptVendor: "gpt" | "deepseek" | "kimi" | "glm";
  baseUrl: string;
  apiKey: string;
  clearApiKey: boolean;
  enabled: boolean;
  supportsImages: boolean;
  supportsReasoning: boolean;
}

export interface AiModelDraft {
  id: string;
  taskUsageScenario: string;
  taskThinkingLevel: string;
  temperature: string;
  maxOutputTokens: string;
  taskEnabled: boolean;
}

export interface AiModelConfigurationDraft extends AiModelDefaults {
  modelDrafts: Record<string, AiModelDraft>;
}

export function configurationDraft(snapshot: AiModelSettingsSnapshot): AiModelConfigurationDraft {
  return {
    ...snapshot.settings,
    modelDrafts: Object.fromEntries(snapshot.models.map((model) => [model.id, {
      id: model.id,
      taskUsageScenario: model.taskUsageScenario ?? "",
      taskThinkingLevel: model.taskThinkingLevel ?? "",
      temperature: model.temperature?.toString() ?? "",
      maxOutputTokens: model.maxOutputTokens?.toString() ?? "",
      taskEnabled: model.taskEnabled,
    }])),
  };
}

export function selectableModels(models: AiModelConfig[]) {
  return models.filter((model) => model.enabled && model.hasApiKey);
}

export function validateConfigurationDraft(
  draft: AiModelConfigurationDraft,
  models: AiModelConfig[],
): string | null {
  if (!Number.isInteger(draft.modelRequestMaxRetries)
    || draft.modelRequestMaxRetries < 0
    || draft.modelRequestMaxRetries > 10) {
    return "模型请求最大重试次数必须在 0 到 10 之间。";
  }
  const runnable = new Set(selectableModels(models).map((model) => model.id));
  if (draft.memorySummaryModelId && !runnable.has(draft.memorySummaryModelId)) {
    return "Memory 总结必须使用已启用且配置了密钥的模型。";
  }
  if (draft.approvalModelId && !runnable.has(draft.approvalModelId)) {
    return "本机审批 Agent 必须使用已启用且配置了密钥的模型。";
  }
  for (const model of models) {
    const item = draft.modelDrafts[model.id];
    if (!item) return `${model.name} 的任务配置缺失。`;
    const temperature = item.temperature.trim();
    if (temperature) {
      const value = Number(temperature);
      if (!Number.isFinite(value) || value < 0 || value > 2) {
        return `${model.name} 的 Temperature 必须留空或填写 0 到 2。`;
      }
    }
    const tokens = item.maxOutputTokens.trim();
    if (tokens) {
      const value = Number(tokens);
      if (!Number.isInteger(value) || value <= 0) {
        return `${model.name} 的 Max Tokens 必须留空或填写大于 0 的整数。`;
      }
    }
  }
  return null;
}

export function blankProviderDraft(): AiModelProviderDraft {
  return {
    providerId: null,
    name: "",
    promptVendor: "gpt",
    baseUrl: "https://api.openai.com/v1",
    apiKey: "",
    clearApiKey: false,
    enabled: true,
    supportsImages: false,
    supportsReasoning: false,
  };
}

export function providerDraft(provider: AiModelProvider): AiModelProviderDraft {
  return {
    providerId: provider.id,
    name: provider.name,
    promptVendor: normalizePromptVendor(provider.promptVendor),
    baseUrl: provider.baseUrl,
    apiKey: "",
    clearApiKey: false,
    enabled: provider.enabled,
    supportsImages: provider.supportsImages,
    supportsReasoning: provider.supportsReasoning,
  };
}

function normalizePromptVendor(value: string): AiModelProviderDraft["promptVendor"] {
  return value === "deepseek" || value === "kimi" || value === "glm" ? value : "gpt";
}
