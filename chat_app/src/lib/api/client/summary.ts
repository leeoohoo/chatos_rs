import type {
  SessionSummariesListResponse,
  SessionSummaryJobConfigPayload,
  SessionSummaryJobConfigResponse,
} from './types';
import type { ApiRequestFn } from './workspace';

export const getSessionSummaryJobConfig = (
  request: ApiRequestFn,
  userId?: string,
): Promise<SessionSummaryJobConfigResponse> => {
  const params = userId ? `?user_id=${encodeURIComponent(userId)}` : '';
  return request<SessionSummaryJobConfigResponse>(`/session-summary-job-config${params}`);
};

export const updateSessionSummaryJobConfig = (
  request: ApiRequestFn,
  payload: SessionSummaryJobConfigPayload,
): Promise<SessionSummaryJobConfigResponse> => {
  return request<SessionSummaryJobConfigResponse>('/session-summary-job-config', {
    method: 'PUT',
    body: JSON.stringify(payload),
  });
};

export const patchSessionSummaryJobConfig = (
  request: ApiRequestFn,
  payload: SessionSummaryJobConfigPayload,
): Promise<SessionSummaryJobConfigResponse> => {
  return request<SessionSummaryJobConfigResponse>('/session-summary-job-config', {
    method: 'PATCH',
    body: JSON.stringify(payload),
  });
};

export const getSessionSummaries = async (
  request: ApiRequestFn,
  sessionId: string,
  options?: { limit?: number; offset?: number }
): Promise<SessionSummariesListResponse> => {
  if (!sessionId) {
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
    `/sessions/${encodeURIComponent(sessionId)}/summaries${query ? `?${query}` : ''}`
  );

  return {
    items: Array.isArray(result?.items) ? result.items : [],
    total: typeof result?.total === 'number' ? result.total : 0,
    has_summary: result?.has_summary === true,
  };
};

export const deleteSessionSummary = (
  request: ApiRequestFn,
  sessionId: string,
  summaryId: string
): Promise<{ success?: boolean }> => {
  if (!sessionId) {
    throw new Error('sessionId is required');
  }
  if (!summaryId) {
    throw new Error('summaryId is required');
  }

  return request<{ success?: boolean }>(
    `/sessions/${encodeURIComponent(sessionId)}/summaries/${encodeURIComponent(summaryId)}`,
    { method: 'DELETE' }
  );
};

export const clearSessionSummaries = (
  request: ApiRequestFn,
  sessionId: string,
): Promise<{ success?: boolean }> => {
  if (!sessionId) {
    throw new Error('sessionId is required');
  }

  return request<{ success?: boolean }>(`/sessions/${encodeURIComponent(sessionId)}/summaries`, {
    method: 'DELETE',
  });
};
