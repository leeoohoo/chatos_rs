import type { SessionAiSelection } from '../types';
import {
  mergeSessionRuntimeIntoMetadata,
  readSessionRuntimeFromMetadata,
} from './sessionRuntime';

const SESSION_AI_SELECTION_KEY = 'ui_chat_selection';

export const readSessionAiSelectionFromMetadata = (
  metadata: any,
): SessionAiSelection | null => {
  const runtime = readSessionRuntimeFromMetadata(metadata);
  if (!runtime?.selectedModelId && !runtime?.contactAgentId) {
    return null;
  }
  return {
    selectedModelId: runtime.selectedModelId,
    selectedAgentId: runtime.contactAgentId,
  };
};

export const mergeSessionAiSelectionIntoMetadata = (
  metadata: any,
  selection: SessionAiSelection,
): Record<string, any> => {
  const next = mergeSessionRuntimeIntoMetadata(metadata, {
    selectedModelId: selection.selectedModelId ?? null,
    contactAgentId: selection.selectedAgentId ?? null,
  }) as Record<string, any>;

  const uiChatSelection = next[SESSION_AI_SELECTION_KEY];
  if (
    uiChatSelection
    && typeof uiChatSelection === 'object'
    && !Array.isArray(uiChatSelection)
    && (uiChatSelection as Record<string, unknown>).selected_model_id == null
    && (uiChatSelection as Record<string, unknown>).selected_agent_id == null
  ) {
    delete next[SESSION_AI_SELECTION_KEY];
  }

  return next;
};
