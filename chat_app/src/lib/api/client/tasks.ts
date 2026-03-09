import type { ApiRequestFn } from './workspace';

export const getTaskManagerTasks = async (
  request: ApiRequestFn,
  sessionId: string,
  options?: { conversationTurnId?: string; includeDone?: boolean; limit?: number }
): Promise<any[]> => {
  if (!sessionId) {
    return [];
  }

  const params = new URLSearchParams();
  params.set('session_id', sessionId);
  if (options?.conversationTurnId) {
    params.set('conversation_turn_id', options.conversationTurnId);
  }
  if (options?.includeDone === true) {
    params.set('include_done', 'true');
  }
  if (typeof options?.limit === 'number') {
    params.set('limit', String(options.limit));
  }

  const result = await request<any>('/task-manager/tasks?' + params.toString());
  if (Array.isArray(result)) {
    return result;
  }
  return Array.isArray(result?.tasks) ? result.tasks : [];
};

export const updateTaskManagerTask = (
  request: ApiRequestFn,
  sessionId: string,
  taskId: string,
  payload: {
    title?: string;
    details?: string;
    priority?: 'high' | 'medium' | 'low';
    status?: 'todo' | 'doing' | 'blocked' | 'done';
    tags?: string[];
    due_at?: string | null;
  }
): Promise<any> => {
  if (!sessionId) {
    throw new Error('sessionId is required');
  }
  if (!taskId) {
    throw new Error('taskId is required');
  }

  const params = new URLSearchParams();
  params.set('session_id', sessionId);
  return request<any>('/task-manager/tasks/' + encodeURIComponent(taskId) + '?' + params.toString(), {
    method: 'PATCH',
    body: JSON.stringify(payload),
  });
};

export const completeTaskManagerTask = (
  request: ApiRequestFn,
  sessionId: string,
  taskId: string
): Promise<any> => {
  if (!sessionId) {
    throw new Error('sessionId is required');
  }
  if (!taskId) {
    throw new Error('taskId is required');
  }

  const params = new URLSearchParams();
  params.set('session_id', sessionId);
  return request<any>('/task-manager/tasks/' + encodeURIComponent(taskId) + '/complete?' + params.toString(), {
    method: 'POST',
    body: JSON.stringify({}),
  });
};

export const deleteTaskManagerTask = (
  request: ApiRequestFn,
  sessionId: string,
  taskId: string
): Promise<any> => {
  if (!sessionId) {
    throw new Error('sessionId is required');
  }
  if (!taskId) {
    throw new Error('taskId is required');
  }

  const params = new URLSearchParams();
  params.set('session_id', sessionId);
  return request<any>('/task-manager/tasks/' + encodeURIComponent(taskId) + '?' + params.toString(), {
    method: 'DELETE',
  });
};

export const submitTaskReviewDecision = (
  request: ApiRequestFn,
  reviewId: string,
  payload: { action: 'confirm' | 'cancel'; tasks?: any[]; reason?: string }
): Promise<any> => {
  if (!reviewId) {
    throw new Error('reviewId is required');
  }

  return request<any>(`/task-manager/reviews/${encodeURIComponent(reviewId)}/decision`, {
    method: 'POST',
    body: JSON.stringify(payload),
  });
};

export const getPendingUiPrompts = async (
  request: ApiRequestFn,
  sessionId: string,
  options?: { limit?: number }
): Promise<any[]> => {
  if (!sessionId) {
    return [];
  }

  const params = new URLSearchParams();
  params.set('session_id', sessionId);
  if (typeof options?.limit === 'number') {
    params.set('limit', String(options.limit));
  }

  const result = await request<any>('/ui-prompts/pending?' + params.toString());
  if (Array.isArray(result)) {
    return result;
  }
  return Array.isArray(result?.prompts) ? result.prompts : [];
};

export const getUiPromptHistory = async (
  request: ApiRequestFn,
  sessionId: string,
  options?: { limit?: number; includePending?: boolean }
): Promise<any[]> => {
  if (!sessionId) {
    return [];
  }

  const params = new URLSearchParams();
  params.set('session_id', sessionId);
  if (typeof options?.limit === 'number') {
    params.set('limit', String(options.limit));
  }
  if (options?.includePending === true) {
    params.set('include_pending', 'true');
  }

  const result = await request<any>('/ui-prompts/history?' + params.toString());
  if (Array.isArray(result)) {
    return result;
  }
  return Array.isArray(result?.prompts) ? result.prompts : [];
};

export const submitUiPromptResponse = (
  request: ApiRequestFn,
  promptId: string,
  payload: {
    status: 'ok' | 'canceled' | 'timeout';
    values?: Record<string, string>;
    selection?: string | string[];
    reason?: string;
  }
): Promise<any> => {
  if (!promptId) {
    throw new Error('promptId is required');
  }

  return request<any>(`/ui-prompts/${encodeURIComponent(promptId)}/respond`, {
    method: 'POST',
    body: JSON.stringify(payload),
  });
};
