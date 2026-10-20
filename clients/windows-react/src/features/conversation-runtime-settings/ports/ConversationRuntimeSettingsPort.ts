import type { ConversationRuntimeSettingsSnapshot } from "../domain/conversationRuntimeSettings";

export interface RuntimeSettingsScope {
  ownerUserId: string;
  conversationId: string;
}

export interface ConversationRuntimeSettingsPort {
  load(scope: RuntimeSettingsScope): Promise<ConversationRuntimeSettingsSnapshot>;
  selectModel(
    scope: RuntimeSettingsScope & { modelId: string },
  ): Promise<ConversationRuntimeSettingsSnapshot>;
  selectReasoningLevel(
    scope: RuntimeSettingsScope & { level: string },
  ): Promise<ConversationRuntimeSettingsSnapshot>;
  selectRemoteConnection(
    scope: RuntimeSettingsScope & { connectionId: string | null },
  ): Promise<ConversationRuntimeSettingsSnapshot>;
}
