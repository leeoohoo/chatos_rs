import type {
  AiModelConfigurationDraft,
  AiModelProviderDraft,
  AiModelSettingsSnapshot,
} from "../domain/aiModelSettings";

export interface AiModelSettingsPort {
  load(ownerUserId: string): Promise<AiModelSettingsSnapshot>;
  save(ownerUserId: string, draft: AiModelConfigurationDraft): Promise<AiModelSettingsSnapshot>;
  createProvider(ownerUserId: string, draft: AiModelProviderDraft): Promise<AiModelSettingsSnapshot>;
  updateProvider(ownerUserId: string, draft: AiModelProviderDraft): Promise<AiModelSettingsSnapshot>;
  refreshProvider(ownerUserId: string, providerId: string): Promise<AiModelSettingsSnapshot>;
  deleteProvider(ownerUserId: string, providerId: string): Promise<AiModelSettingsSnapshot>;
}
