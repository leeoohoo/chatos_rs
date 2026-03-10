import type { ApiRequestFn } from './workspace';

export const getSessionSummaryJobConfig = (request: ApiRequestFn, userId?: string): Promise<any> => {
  const params = userId ? `?user_id=${encodeURIComponent(userId)}` : '';
  return request<any>(`/session-summary-job-config${params}`);
};

export const updateSessionSummaryJobConfig = (
  request: ApiRequestFn,
  payload: {
    user_id?: string;
    enabled?: boolean;
    summary_model_config_id?: string | null;
    token_limit?: number;
    message_count_limit?: number;
    round_limit?: number;
    target_summary_tokens?: number;
    job_interval_seconds?: number;
  }
): Promise<any> => {
  return request<any>('/session-summary-job-config', {
    method: 'PUT',
    body: JSON.stringify(payload),
  });
};

export const patchSessionSummaryJobConfig = (
  request: ApiRequestFn,
  payload: {
    user_id?: string;
    enabled?: boolean;
    summary_model_config_id?: string | null;
    token_limit?: number;
    message_count_limit?: number;
    round_limit?: number;
    target_summary_tokens?: number;
    job_interval_seconds?: number;
  }
): Promise<any> => {
  return request<any>('/session-summary-job-config', {
    method: 'PATCH',
    body: JSON.stringify(payload),
  });
};

export const getSessionSummaries = async (
  request: ApiRequestFn,
  sessionId: string,
  options?: { limit?: number; offset?: number }
): Promise<{ items: any[]; total: number; has_summary: boolean }> => {
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
  const result = await request<any>(
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
): Promise<any> => {
  if (!sessionId) {
    throw new Error('sessionId is required');
  }
  if (!summaryId) {
    throw new Error('summaryId is required');
  }

  return request<any>(
    `/sessions/${encodeURIComponent(sessionId)}/summaries/${encodeURIComponent(summaryId)}`,
    { method: 'DELETE' }
  );
};

export const clearSessionSummaries = (request: ApiRequestFn, sessionId: string): Promise<any> => {
  if (!sessionId) {
    throw new Error('sessionId is required');
  }

  return request<any>(`/sessions/${encodeURIComponent(sessionId)}/summaries`, {
    method: 'DELETE',
  });
};
