// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Session } from '../../types';
import type { SessionResponse } from '../api/client/types';
import type ApiClient from '../api/client';
import {
  asRecord,
  normalizeDate,
  readBoolean,
  readTrimmedString,
  readValue,
} from './normalizerUtils';

export const normalizeSession = (raw: SessionResponse | Session | unknown): Session => {
  const record = asRecord(raw);
  const metadataRecord = asRecord(readValue(record, 'metadata'));
  const chatRuntimeRecord = asRecord(metadataRecord?.chat_runtime);
  const contactRecord = asRecord(metadataRecord?.contact);

  const statusValue = readValue(record, 'status');
  const status = typeof statusValue === 'string'
    ? statusValue.toLowerCase()
    : (readBoolean(record, 'archived') ? 'archived' : 'active');
  const archived = readBoolean(record, 'archived') === true || status === 'archiving' || status === 'archived';
  const rawProjectId = readTrimmedString(record, 'project_id')
    || readTrimmedString(record, 'projectId');
  const metadataProjectId = readTrimmedString(chatRuntimeRecord, 'project_id')
    || readTrimmedString(chatRuntimeRecord, 'projectId');
  const selectedProjectId = rawProjectId.length > 0
    ? rawProjectId
    : (metadataProjectId.length > 0 ? metadataProjectId : '');
  const selectedModelId = readTrimmedString(record, 'selected_model_id')
    || readTrimmedString(chatRuntimeRecord, 'selected_model_id');
  const selectedAgentId = readTrimmedString(record, 'selected_agent_id')
    || readTrimmedString(contactRecord, 'agent_id')
    || readTrimmedString(contactRecord, 'agentId')
    || readTrimmedString(chatRuntimeRecord, 'contact_agent_id')
    || readTrimmedString(chatRuntimeRecord, 'contactAgentId');
  let metadata = (readValue(record, 'metadata') ?? null) as Session['metadata'];
  const hasSelection = selectedModelId.length > 0
    || selectedAgentId.length > 0
    || selectedProjectId.length > 0;
  if (hasSelection) {
    const metadataObject = metadataRecord
      ? { ...metadataRecord }
      : {};
    metadataObject.ui_chat_selection = {
      selected_model_id: selectedModelId.length > 0 ? selectedModelId : null,
      selected_agent_id: selectedAgentId.length > 0 ? selectedAgentId : null,
    };
    metadataObject.chat_runtime = {
      ...(asRecord(metadataObject.chat_runtime) ?? {}),
      selected_model_id: selectedModelId.length > 0 ? selectedModelId : null,
      contact_agent_id: selectedAgentId.length > 0 ? selectedAgentId : null,
      project_id: selectedProjectId.length > 0 ? selectedProjectId : null,
    };
    metadataObject.contact = {
      ...(asRecord(metadataObject.contact) ?? {}),
      type: 'memory_agent',
      agent_id: selectedAgentId.length > 0 ? selectedAgentId : null,
    };
    metadata = metadataObject;
  }

  return {
    id: (readValue(record, 'id') ?? '') as Session['id'],
    title: (readValue(record, 'title') ?? '') as Session['title'],
    userId: (readValue(record, 'user_id') ?? readValue(record, 'userId') ?? null) as Session['userId'],
    user_id: (readValue(record, 'user_id') ?? readValue(record, 'userId') ?? null) as Session['user_id'],
    projectId: selectedProjectId.length > 0 ? selectedProjectId : null,
    project_id: selectedProjectId.length > 0 ? selectedProjectId : null,
    createdAt: normalizeDate(readValue(record, 'created_at') ?? readValue(record, 'createdAt') ?? Date.now()),
    updatedAt: normalizeDate(
      readValue(record, 'updated_at')
      ?? readValue(record, 'updatedAt')
      ?? readValue(record, 'created_at')
      ?? readValue(record, 'createdAt')
      ?? Date.now(),
    ),
    messageCount: (readValue(record, 'messageCount') ?? readValue(record, 'message_count') ?? 0) as Session['messageCount'],
    tokenUsage: (readValue(record, 'tokenUsage') ?? readValue(record, 'token_usage') ?? 0) as Session['tokenUsage'],
    pinned: (readValue(record, 'pinned') ?? false) as Session['pinned'],
    archived,
    status,
    tags: (readValue(record, 'tags') ?? null) as Session['tags'],
    metadata,
  };
};

export const fetchSession = async (client: ApiClient, sessionId: string): Promise<Session | null> => {
  try {
    const session = await client.getSession(sessionId);
    if (!session) return null;
    return normalizeSession(session);
  } catch {
    return null;
  }
};
