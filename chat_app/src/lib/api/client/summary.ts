import type {
  SessionSummariesListResponse,
  SessionSummaryJobConfigPayload,
  SessionSummaryJobConfigResponse,
} from './types';
import type { ApiRequestFn } from './workspace';

export const getConversationSummaryJobConfig = (
  request: ApiRequestFn,
  userId?: string,
): Promise<SessionSummaryJobConfigResponse> => {
  const params = userId ? `?user_id=${encodeURIComponent(userId)}` : '';
  return request<SessionSummaryJobConfigResponse>(`/session-summary-job-config${params}`);
};

export const updateConversationSummaryJobConfig = (
  request: ApiRequestFn,
  payload: SessionSummaryJobConfigPayload,
): Promise<SessionSummaryJobConfigResponse> => {
  return request<SessionSummaryJobConfigResponse>('/session-summary-job-config', {
    method: 'PUT',
    body: JSON.stringify(payload),
  });
};

export const patchConversationSummaryJobConfig = (
  request: ApiRequestFn,
  payload: SessionSummaryJobConfigPayload,
): Promise<SessionSummaryJobConfigResponse> => {
  return request<SessionSummaryJobConfigResponse>('/session-summary-job-config', {
    method: 'PATCH',
    body: JSON.stringify(payload),
  });
};

export const getConversationSummaries = async (
  request: ApiRequestFn,
  conversationId: string,
  options?: { limit?: number; offset?: number }
): Promise<SessionSummariesListResponse> => {
  if (!conversationId) {
    return { items: [], total: 0, has_summary: false };
  }

  const params = new URLSearchParams();
  if (typeof options?.limit === 'number') {
    params.set('limit', String(options.limit));
  }
  if (typeof options?.offset === 'number') {
    params.set('offset', String(options.offset));
  }
  const query = params.toString();
  const result = await request<Partial<SessionSummariesListResponse>>(
    `/conversations/${encodeURIComponent(conversationId)}/summaries${query ? `?${query}` : ''}`
  );

  return {
    items: Array.isArray(result?.items) ? result.items : [],
    total: typeof result?.total === 'number' ? result.total : 0,
    has_summary: result?.has_summary === true,
  };
};

export const deleteConversationSummary = (
  request: ApiRequestFn,
  conversationId: string,
  summaryId: string
): Promise<{ success?: boolean }> => {
  if (!conversationId) {
    throw new Error('conversationId is required');
  }
  if (!summaryId) {
    throw new Error('summaryId is required');
  }

  return request<{ success?: boolean }>(
    `/conversations/${encodeURIComponent(conversationId)}/summaries/${encodeURIComponent(summaryId)}`,
    { method: 'DELETE' }
  );
};

export const clearConversationSummaries = (
  request: ApiRequestFn,
  conversationId: string,
): Promise<{ success?: boolean }> => {
  if (!conversationId) {
    throw new Error('conversationId is required');
  }

  return request<{ success?: boolean }>(`/conversations/${encodeURIComponent(conversationId)}/summaries`, {
    method: 'DELETE',
  });
};
