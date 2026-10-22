import type { AiModelSettingsSnapshot } from "../features/ai-model-settings/domain/aiModelSettings";
import type { AiModelSettingsPort } from "../features/ai-model-settings/ports/AiModelSettingsPort";
import { vi } from "vitest";

export function aiModelSettingsSnapshot(): AiModelSettingsSnapshot {
  return {
    models: [{
      id: "model-1", sourceProviderId: "provider-1", name: "GPT 5", provider: "gpt",
      promptVendor: "gpt", baseUrl: "https://api.example.com/v1", modelName: "gpt-5",
      taskUsageScenario: "Coding", taskThinkingLevel: "medium", temperature: 0.7,
      maxOutputTokens: 4096, enabled: true, taskEnabled: true, hasApiKey: true,
      supportsImages: true, supportsReasoning: true, supportsResponses: true,
      thinkingLevels: ["none", "low", "medium", "high"],
    }],
    providers: [{
      id: "provider-1", name: "OpenAI", provider: "gpt", promptVendor: "gpt",
      baseUrl: "https://api.example.com/v1", hasApiKey: true, enabled: true,
      supportsImages: true, supportsReasoning: true, supportsResponses: true,
      lastSyncStatus: "success", lastSyncError: null, importedModelCount: 1,
    }],
    settings: {
      modelRequestMaxRetries: 5, memorySummaryModelId: "model-1",
      memorySummaryThinkingLevel: "medium", approvalModelId: "model-1",
      approvalThinkingLevel: "medium",
    },
  };
}

export function aiModelSettingsPort(overrides: Partial<AiModelSettingsPort> = {}): AiModelSettingsPort {
  return {
    load: vi.fn(async () => aiModelSettingsSnapshot()),
    save: vi.fn(async () => aiModelSettingsSnapshot()),
    createProvider: vi.fn(async () => aiModelSettingsSnapshot()),
    updateProvider: vi.fn(async () => aiModelSettingsSnapshot()),
    refreshProvider: vi.fn(async () => aiModelSettingsSnapshot()),
    deleteProvider: vi.fn(async () => aiModelSettingsSnapshot()),
    ...overrides,
  };
}
