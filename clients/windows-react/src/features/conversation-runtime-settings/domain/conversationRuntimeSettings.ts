export interface ConversationModelOption {
  id: string;
  displayName: string;
  modelName: string;
  provider: string;
  defaultThinkingLevel: string;
  supportsReasoning: boolean;
  thinkingLevels: string[];
}

export interface RemoteConnectionOption {
  id: string;
  name: string;
}

export interface ConversationRuntimeSettings {
  selectedModelId: string;
  selectedThinkingLevel: string | null;
  remoteConnectionId: string | null;
  reasoningEnabled: boolean;
  version: number;
}

export interface ConversationRuntimeSettingsSnapshot {
  settings: ConversationRuntimeSettings;
  models: ConversationModelOption[];
  remoteConnections: RemoteConnectionOption[];
}

export function selectedModel(
  snapshot: ConversationRuntimeSettingsSnapshot,
): ConversationModelOption | null {
  return snapshot.models.find((model) => model.id === snapshot.settings.selectedModelId)
    ?? snapshot.models[0]
    ?? null;
}

export function effectiveThinkingLevel(
  snapshot: ConversationRuntimeSettingsSnapshot,
): string {
  const model = selectedModel(snapshot);
  if (!model?.supportsReasoning || !snapshot.settings.reasoningEnabled) return "none";
  const selected = snapshot.settings.selectedThinkingLevel;
  if (selected && model.thinkingLevels.includes(selected)) return selected;
  return model.defaultThinkingLevel;
}
