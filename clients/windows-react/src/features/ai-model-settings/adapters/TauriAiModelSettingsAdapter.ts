import { invoke } from "@tauri-apps/api/core";
import {
  aiModelSettingsSnapshotSchema,
  type AiModelConfigurationDraft,
  type AiModelProviderDraft,
} from "../domain/aiModelSettings";
import type { AiModelSettingsPort } from "../ports/AiModelSettingsPort";

export class TauriAiModelSettingsAdapter implements AiModelSettingsPort {
  async load(ownerUserId: string) {
    return parse(await invoke("load_ai_model_settings", { ownerUserId }));
  }

  async save(ownerUserId: string, draft: AiModelConfigurationDraft) {
    return parse(await invoke("save_ai_model_configuration", {
      input: {
        ownerUserId,
        modelRequestMaxRetries: draft.modelRequestMaxRetries,
        memorySummaryModelId: draft.memorySummaryModelId,
        memorySummaryThinkingLevel: draft.memorySummaryThinkingLevel,
        approvalModelId: draft.approvalModelId,
        approvalThinkingLevel: draft.approvalThinkingLevel,
        modelUpdates: Object.values(draft.modelDrafts).map((model) => ({
          id: model.id,
          taskUsageScenario: model.taskUsageScenario.trim() || null,
          taskThinkingLevel: model.taskThinkingLevel || null,
          temperature: model.temperature.trim() ? Number(model.temperature) : null,
          maxOutputTokens: model.maxOutputTokens.trim() ? Number(model.maxOutputTokens) : null,
          taskEnabled: model.taskEnabled,
        })),
      },
    }));
  }

  async createProvider(ownerUserId: string, draft: AiModelProviderDraft) {
    return parse(await invoke("create_ai_model_provider", { input: providerInput(ownerUserId, draft) }));
  }

  async updateProvider(ownerUserId: string, draft: AiModelProviderDraft) {
    return parse(await invoke("update_ai_model_provider", { input: providerInput(ownerUserId, draft) }));
  }

  async refreshProvider(ownerUserId: string, providerId: string) {
    return parse(await invoke("refresh_ai_model_provider", { input: { ownerUserId, providerId } }));
  }

  async deleteProvider(ownerUserId: string, providerId: string) {
    return parse(await invoke("delete_ai_model_provider", { input: { ownerUserId, providerId } }));
  }
}

function providerInput(ownerUserId: string, draft: AiModelProviderDraft) {
  return {
    ownerUserId,
    providerId: draft.providerId,
    name: draft.name.trim(),
    promptVendor: draft.promptVendor,
    baseUrl: draft.baseUrl.trim(),
    apiKey: draft.apiKey.trim() || null,
    clearApiKey: draft.clearApiKey,
    enabled: draft.enabled,
    supportsImages: draft.supportsImages,
    supportsReasoning: draft.supportsReasoning,
  };
}

function parse(value: unknown) {
  return aiModelSettingsSnapshotSchema.parse(value);
}
