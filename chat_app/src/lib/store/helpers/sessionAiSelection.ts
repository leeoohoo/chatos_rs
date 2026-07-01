// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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

const getSessionMetadataSource = (metadata: unknown): Record<string, unknown> => {
  const meta = parseSessionMetadata(metadata);
  const source = asRecord(readValue(meta, 'source_metadata'));
  return source ?? meta;
};

const getMutableSessionMetadataSource = (metadata: Record<string, unknown>): Record<string, unknown> => {
  const source = asRecord(readValue(metadata, 'source_metadata'));
  return source
    ? { ...source }
    : { ...metadata };
};

export const readSessionAiSelectionFromMetadata = (
  metadata: unknown,
): SessionAiSelection | null => {
  const meta = getSessionMetadataSource(metadata);
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
  const source = getMutableSessionMetadataSource(next);
  const selectedModelId = normalizeId(selection.selectedModelId);
  const selectedAgentId = normalizeId(selection.selectedAgentId);

  if (!selectedModelId && !selectedAgentId) {
    delete source[SESSION_AI_SELECTION_KEY];
    if (asRecord(next.source_metadata)) {
      next.source_metadata = source;
    } else {
      Object.assign(next, source);
    }
    return next;
  }

  source[SESSION_AI_SELECTION_KEY] = {
    selected_model_id: selectedModelId,
    selected_agent_id: selectedAgentId,
  };
  source.chat_runtime = {
    ...(asRecord(source.chat_runtime) ?? {}),
    selected_model_id: selectedModelId,
    contact_agent_id: selectedAgentId,
  };
  source.contact = {
    ...(asRecord(source.contact) ?? {}),
    type: 'memory_agent',
    agent_id: selectedAgentId,
  };
  source.ui_contact = {
    type: 'memory_agent',
    agent_id: selectedAgentId,
  };

  if (asRecord(next.source_metadata)) {
    next.source_metadata = source;
  } else {
    Object.assign(next, source);
  }
  return next;
};
