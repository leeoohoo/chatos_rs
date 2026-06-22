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

export const getConversationUserMessageTurns = (
  request: ApiRequestFn,
  conversationId: string,
  params?: { limit?: number; before?: string | null },
): Promise<UserMessageTurnsResponse> => {
  const query = buildQuery({
    limit: params?.limit,
    before: params?.before,
  });
  return request<UserMessageTurnsResponse>(
    `/conversations/${conversationId}/user-message-turns${query}`,
  );
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
