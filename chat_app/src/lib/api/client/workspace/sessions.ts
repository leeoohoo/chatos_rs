import { debugLog } from '@/lib/utils';

import { buildQuery } from '../shared';
import type {
  CompactHistoryResponse,
  DeleteSuccessResponse,
  SessionMessageResponse,
  SessionResponse,
  TurnRuntimeSnapshotLookupResponse,
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

export const getConversationTurnProcessMessages = (
  request: ApiRequestFn,
  conversationId: string,
  userMessageId: string,
): Promise<SessionMessageResponse[]> => {
  return request<SessionMessageResponse[]>(
    `/conversations/${conversationId}/turns/${encodeURIComponent(userMessageId)}/process`,
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

export const getConversationTurnProcessMessagesByTurn = (
  request: ApiRequestFn,
  conversationId: string,
  turnId: string,
): Promise<SessionMessageResponse[]> => {
  return request<SessionMessageResponse[]>(
    `/conversations/${conversationId}/turns/by-turn/${encodeURIComponent(turnId)}/process`,
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
