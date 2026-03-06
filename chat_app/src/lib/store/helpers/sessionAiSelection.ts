import type { SessionAiSelection } from '../types';

const SESSION_AI_SELECTION_KEY = 'ui_chat_selection';

const normalizeId = (value: unknown): string | null => {
  if (typeof value !== 'string') {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
};

const parseSessionMetadata = (metadata: any): Record<string, any> => {
  if (metadata && typeof metadata === 'object' && !Array.isArray(metadata)) {
    return { ...metadata };
  }
  if (typeof metadata === 'string') {
    try {
      const parsed = JSON.parse(metadata);
      if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
        return parsed;
      }
    } catch {
      // ignore parse errors and fallback to empty object
    }
  }
  return {};
};

export const readSessionAiSelectionFromMetadata = (
  metadata: any,
): SessionAiSelection | null => {
  const meta = parseSessionMetadata(metadata);
  const raw = meta?.[SESSION_AI_SELECTION_KEY];
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) {
    return null;
  }
  const selectedModelId = normalizeId((raw as any).selected_model_id ?? (raw as any).selectedModelId);
  const selectedAgentId = normalizeId((raw as any).selected_agent_id ?? (raw as any).selectedAgentId);
  if (!selectedModelId && !selectedAgentId) {
    return null;
  }
  return { selectedModelId, selectedAgentId };
};

export const mergeSessionAiSelectionIntoMetadata = (
  metadata: any,
  selection: SessionAiSelection,
): Record<string, any> => {
  const next = parseSessionMetadata(metadata);
  const selectedModelId = normalizeId(selection.selectedModelId);
  const selectedAgentId = normalizeId(selection.selectedAgentId);

  if (!selectedModelId && !selectedAgentId) {
    delete next[SESSION_AI_SELECTION_KEY];
    return next;
  }

  next[SESSION_AI_SELECTION_KEY] = {
    selected_model_id: selectedModelId,
    selected_agent_id: selectedAgentId,
  };
  return next;
};
