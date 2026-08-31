// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { debugLog } from '@/lib/utils';

import { buildQuery } from '../shared';
import type {
  CompactHistoryResponse,
  DeleteSuccessResponse,
  SessionMessageResponse,
  SessionResponse,
  SessionRuntimeSettingsPayload,
  SessionRuntimeSettingsResponse,
  ConversationTaskRunnerActiveMessageTasksResponse,
  TurnRuntimeSnapshotLookupResponse,
  UserMessageTurnResponse,
  UserMessageTurnsResponse,
} from '../types';
import type { ApiRequestFn, SessionPaging } from './common';

export const getSessions = (
  request: ApiRequestFn,
  userId?: string,
  projectId?: string,
  paging?: SessionPaging,
): Promise<SessionResponse[]> => {
  const query = buildQuery({
    user_id: userId,
    project_id: projectId,
    limit: paging?.limit,
    offset: paging?.offset,
    include_archived: paging?.includeArchived === true ? true : undefined,
    include_archiving: paging?.includeArchiving === true ? true : undefined,
  });
  debugLog('🔍 getSessions API调用:', { userId, projectId, query });
  return request<SessionResponse[]>(`/conversations${query}`);
};

export const createSession = (
  request: ApiRequestFn,
  data: {
    id: string;
    title: string;
    user_id: string;
    project_id?: string;
    metadata?: Record<string, unknown> | string | null;
  },
): Promise<SessionResponse> => {
  debugLog('🔍 createSession API调用:', data);
  return request<SessionResponse>('/conversations', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const getSession = (request: ApiRequestFn, id: string): Promise<SessionResponse> => {
  return request<SessionResponse>(`/conversations/${id}`);
};

export const updateSession = (
  request: ApiRequestFn,
  id: string,
  data: { title?: string; description?: string; metadata?: Record<string, unknown> | string | null },
): Promise<SessionResponse> => {
  return request<SessionResponse>(`/conversations/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
};

export const getConversationRuntimeSettings = (
  request: ApiRequestFn,
  conversationId: string,
): Promise<SessionRuntimeSettingsResponse> => {
  return request<SessionRuntimeSettingsResponse>(
    `/conversations/${conversationId}/runtime-settings`,
  );
};

export const updateConversationRuntimeSettings = (
  request: ApiRequestFn,
  conversationId: string,
  data: SessionRuntimeSettingsPayload,
): Promise<SessionRuntimeSettingsResponse> => {
  return request<SessionRuntimeSettingsResponse>(
    `/conversations/${conversationId}/runtime-settings`,
    {
      method: 'PUT',
      body: JSON.stringify(data),
    },
  );
};

export const deleteSession = (request: ApiRequestFn, id: string): Promise<DeleteSuccessResponse> => {
  return request<DeleteSuccessResponse>(`/conversations/${id}`, {
    method: 'DELETE',
  });
};

export const getConversationMessages = (
  request: ApiRequestFn,
  conversationId: string,
  params?: { limit?: number; offset?: number; compact?: boolean; strategy?: string },
): Promise<SessionMessageResponse[]> => {
  const query = buildQuery({
    limit: params?.limit,
    offset: params?.offset,
    compact: params?.compact,
    strategy: params?.strategy,
  });
  return request<SessionMessageResponse[]>(`/conversations/${conversationId}/messages${query}`);
};

export const getConversationCompactHistory = (
  request: ApiRequestFn,
  conversationId: string,
  params?: { limit?: number; before?: string | null },
): Promise<CompactHistoryResponse> => {
  const query = buildQuery({
    limit: params?.limit,
    before: params?.before,
  });
  return request<CompactHistoryResponse>(`/conversations/${conversationId}/compact-history${query}`);
};

const readRecord = (value: unknown): Record<string, unknown> | null => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null
);

const readString = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

const messageMetadata = (message: SessionMessageResponse): Record<string, unknown> => (
  readRecord(message.metadata) || {}
);

export const compactHistoryToUserMessageTurns = (
  response: CompactHistoryResponse,
): UserMessageTurnsResponse => {
  const messages = Array.isArray(response.items) ? response.items : [];
  const assistantsById = new Map<string, SessionMessageResponse>();
  const assistantsByUserMessageId = new Map<string, SessionMessageResponse>();
  const assistantsByTurnId = new Map<string, SessionMessageResponse>();

  messages.forEach((message) => {
    if (message.role !== 'assistant') {
      return;
    }
    const id = readString(message.id);
    if (id) {
      assistantsById.set(id, message);
    }
    const metadata = messageMetadata(message);
    const userMessageId = readString(metadata['historyFinalForUserMessageId']);
    if (userMessageId) {
      assistantsByUserMessageId.set(userMessageId, message);
    }
    const taskRunnerAsync = readRecord(metadata['task_runner_async']);
    const turnId = readString(metadata['historyFinalForTurnId'])
      || readString(metadata['conversation_turn_id'])
      || readString(taskRunnerAsync?.['source_turn_id']);
    if (turnId) {
      assistantsByTurnId.set(turnId, message);
    }
  });

  const items = messages.flatMap<UserMessageTurnResponse>((message) => {
    if (message.role !== 'user') {
      return [];
    }
    const metadata = messageMetadata(message);
    const historyProcess = readRecord(metadata['historyProcess']);
    const taskRunnerAsync = readRecord(metadata['task_runner_async']);
    const turnId = readString(historyProcess?.['turnId'])
      || readString(metadata['conversation_turn_id'])
      || readString(taskRunnerAsync?.['source_turn_id'])
      || readString(message.id);
    const finalAssistantId = readString(historyProcess?.['finalAssistantMessageId']);
    const finalAssistantMessage = (finalAssistantId
      ? assistantsById.get(finalAssistantId)
      : undefined)
      || assistantsByUserMessageId.get(readString(message.id))
      || assistantsByTurnId.get(turnId)
      || null;

    return [{
      turn_id: turnId,
      user_message: message,
      final_assistant_message: finalAssistantMessage,
      has_process: historyProcess?.['hasProcess'] === true,
      tool_call_count: Number(historyProcess?.['toolCallCount'] || 0),
      thinking_count: Number(historyProcess?.['thinkingCount'] || 0),
      process_message_count: Number(historyProcess?.['processMessageCount'] || 0),
    }];
  });

  return {
    items,
    has_more: response.has_more === true,
    next_before: response.next_before || null,
  };
};

export const getConversationUserMessageTurns = (
  request: ApiRequestFn,
  conversationId: string,
  params?: { limit?: number; before?: string | null },
): Promise<UserMessageTurnsResponse> => {
  return getConversationCompactHistory(request, conversationId, params)
    .then(compactHistoryToUserMessageTurns);
};

export const getConversationTaskRunnerActiveMessageTasks = (
  request: ApiRequestFn,
  conversationId: string,
  params?: { sourceUserMessageIds?: string[]; sourceTurnIds?: string[] },
): Promise<ConversationTaskRunnerActiveMessageTasksResponse> => {
  return request<ConversationTaskRunnerActiveMessageTasksResponse>(
    `/conversations/${conversationId}/task-runner/active-message-tasks`,
    {
      method: 'POST',
      body: JSON.stringify({
        source_user_message_ids: params?.sourceUserMessageIds || [],
        source_turn_ids: params?.sourceTurnIds || [],
      }),
    },
  );
};

export const getConversationTurnMessages = (
  request: ApiRequestFn,
  conversationId: string,
  userMessageId: string,
): Promise<SessionMessageResponse[]> => {
  return request<SessionMessageResponse[]>(
    `/conversations/${conversationId}/turns/${encodeURIComponent(userMessageId)}/messages`,
  );
};

export const getConversationTurnMessagesByTurn = (
  request: ApiRequestFn,
  conversationId: string,
  turnId: string,
): Promise<SessionMessageResponse[]> => {
  return request<SessionMessageResponse[]>(
    `/conversations/${conversationId}/turns/by-turn/${encodeURIComponent(turnId)}/messages`,
  );
};

export const getConversationLatestTurnRuntimeContext = (
  request: ApiRequestFn,
  conversationId: string,
): Promise<TurnRuntimeSnapshotLookupResponse> => {
  return request<TurnRuntimeSnapshotLookupResponse>(
    `/conversations/${conversationId}/turns/latest/runtime-context`,
  );
};

export const getConversationTurnRuntimeContextByTurn = (
  request: ApiRequestFn,
  conversationId: string,
  turnId: string,
): Promise<TurnRuntimeSnapshotLookupResponse> => {
  return request<TurnRuntimeSnapshotLookupResponse>(
    `/conversations/${conversationId}/turns/by-turn/${encodeURIComponent(turnId)}/runtime-context`,
  );
};
