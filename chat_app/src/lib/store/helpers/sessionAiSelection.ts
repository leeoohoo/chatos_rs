import type { SessionAiSelection } from '../types';
import {
  asRecord,
  readValue,
} from './normalizerUtils';

const SESSION_AI_SELECTION_KEY = 'ui_chat_selection';

const normalizeId = (value: unknown): string | null => {
  if (typeof value !== 'string') {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
};

const parseSessionMetadata = (metadata: unknown): Record<string, unknown> => {
  const metadataRecord = asRecord(metadata);
  if (metadataRecord) {
    return { ...metadataRecord };
  }
  if (typeof metadata === 'string') {
    try {
      const parsed = JSON.parse(metadata);
      const parsedRecord = asRecord(parsed);
      if (parsedRecord) {
        return parsedRecord;
      }
    } catch {
      // ignore parse errors and fallback to empty object
    }
  }
  return {};
};

export const readSessionAiSelectionFromMetadata = (
  metadata: unknown,
): SessionAiSelection | null => {
  const meta = parseSessionMetadata(metadata);
  const raw = asRecord(readValue(meta, SESSION_AI_SELECTION_KEY));
  const runtime = asRecord(readValue(meta, 'chat_runtime'));
  const contact = asRecord(readValue(meta, 'contact'));
  const uiContact = asRecord(readValue(meta, 'ui_contact'));
  const selectedModelId = normalizeId(
    readValue(runtime, 'selected_model_id')
      ?? readValue(runtime, 'selectedModelId')
      ?? readValue(raw, 'selected_model_id')
      ?? readValue(raw, 'selectedModelId'),
  );
  const selectedAgentId = normalizeId(
    readValue(contact, 'agent_id')
      ?? readValue(contact, 'agentId')
      ?? readValue(runtime, 'contact_agent_id')
      ?? readValue(runtime, 'contactAgentId')
      ?? readValue(uiContact, 'agent_id')
      ?? readValue(uiContact, 'agentId')
      ?? readValue(raw, 'selected_agent_id')
      ?? readValue(raw, 'selectedAgentId'),
  );
  if (!selectedModelId && !selectedAgentId) {
    return null;
  }
  return { selectedModelId, selectedAgentId };
};

export const mergeSessionAiSelectionIntoMetadata = (
  metadata: unknown,
  selection: SessionAiSelection,
): Record<string, unknown> => {
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
  next.chat_runtime = {
    ...(asRecord(next.chat_runtime) ?? {}),
    selected_model_id: selectedModelId,
    contact_agent_id: selectedAgentId,
  };
  next.contact = {
    ...(asRecord(next.contact) ?? {}),
    type: 'memory_agent',
    agent_id: selectedAgentId,
  };
  next.ui_contact = {
    type: 'memory_agent',
    agent_id: selectedAgentId,
  };
  return next;
};
