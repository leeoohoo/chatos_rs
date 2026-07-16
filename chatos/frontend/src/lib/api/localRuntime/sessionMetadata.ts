// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

type MetadataRecord = Record<string, unknown>;

export interface LocalSessionSelection {
  metadata: MetadataRecord;
  selectedModelId: string | null;
  selectedAgentId: string | null;
}

export const readLocalSessionSelection = (
  metadata: MetadataRecord | string | null | undefined,
): LocalSessionSelection => {
  const record = parseMetadata(metadata);
  const uiSelection = asRecord(record.ui_chat_selection);
  const chatRuntime = asRecord(record.chat_runtime);
  return {
    metadata: record,
    selectedModelId: readString(uiSelection, 'selected_model_id', 'selectedModelId')
      || readString(chatRuntime, 'selected_model_id', 'selectedModelId'),
    selectedAgentId: readString(uiSelection, 'selected_agent_id', 'selectedAgentId')
      || readString(chatRuntime, 'contact_agent_id', 'contactAgentId'),
  };
};

const parseMetadata = (
  metadata: MetadataRecord | string | null | undefined,
): MetadataRecord => {
  if (metadata && typeof metadata === 'object' && !Array.isArray(metadata)) {
    return metadata;
  }
  if (typeof metadata !== 'string' || !metadata.trim()) {
    return {};
  }
  try {
    return asRecord(JSON.parse(metadata)) || {};
  } catch {
    return {};
  }
};

const asRecord = (value: unknown): MetadataRecord | null => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as MetadataRecord
    : null
);

const readString = (
  record: MetadataRecord | null,
  ...keys: string[]
): string | null => {
  for (const key of keys) {
    const value = record?.[key];
    if (typeof value === 'string' && value.trim()) {
      return value.trim();
    }
  }
  return null;
};
