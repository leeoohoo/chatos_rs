import type {
  ImActionRequestSubmitResponse,
  ImConversationCreatePayload,
  ImConversationActionRequestResponse,
  ImConversationResponse,
  ImConversationMessageCreatePayload,
  ImConversationMessageResponse,
  ImConversationRunResponse,
  TaskManagerTaskResponse,
  TaskManagerUpdatePayload,
  TaskReviewDecisionPayload,
  UiPromptItemResponse,
  UiPromptResponsePayload,
} from './types';
import type { ApiRequestFn } from './workspace';

export const getTaskManagerTasks = async (
  request: ApiRequestFn,
  sessionId: string,
  options?: { conversationTurnId?: string; includeDone?: boolean; limit?: number }
): Promise<TaskManagerTaskResponse[]> => {
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

  const result = await request<{ tasks?: TaskManagerTaskResponse[] } | TaskManagerTaskResponse[]>(
    '/task-manager/tasks?' + params.toString(),
  );
  if (Array.isArray(result)) {
    return result;
  }
  return Array.isArray(result?.tasks) ? result.tasks : [];
};

export const updateTaskManagerTask = (
  request: ApiRequestFn,
  sessionId: string,
  taskId: string,
  payload: TaskManagerUpdatePayload,
): Promise<TaskManagerTaskResponse> => {
  if (!sessionId) {
    throw new Error('sessionId is required');
  }
  if (!taskId) {
    throw new Error('taskId is required');
  }

  const params = new URLSearchParams();
  params.set('session_id', sessionId);
  return request<TaskManagerTaskResponse>(
    '/task-manager/tasks/' + encodeURIComponent(taskId) + '?' + params.toString(),
    {
    method: 'PATCH',
    body: JSON.stringify(payload),
    },
  );
};

export const completeTaskManagerTask = (
  request: ApiRequestFn,
  sessionId: string,
  taskId: string
): Promise<TaskManagerTaskResponse> => {
  if (!sessionId) {
    throw new Error('sessionId is required');
  }
  if (!taskId) {
    throw new Error('taskId is required');
  }

  const params = new URLSearchParams();
  params.set('session_id', sessionId);
  return request<TaskManagerTaskResponse>(
    '/task-manager/tasks/' + encodeURIComponent(taskId) + '/complete?' + params.toString(),
    {
    method: 'POST',
    body: JSON.stringify({}),
    },
  );
};

export const deleteTaskManagerTask = (
  request: ApiRequestFn,
  sessionId: string,
  taskId: string
): Promise<{ success?: boolean }> => {
  if (!sessionId) {
    throw new Error('sessionId is required');
  }
  if (!taskId) {
    throw new Error('taskId is required');
  }

  const params = new URLSearchParams();
  params.set('session_id', sessionId);
  return request<{ success?: boolean }>(
    '/task-manager/tasks/' + encodeURIComponent(taskId) + '?' + params.toString(),
    {
    method: 'DELETE',
    },
  );
};

export const submitTaskReviewDecision = (
  request: ApiRequestFn,
  reviewId: string,
  payload: TaskReviewDecisionPayload,
): Promise<{ success?: boolean; status?: string }> => {
  if (!reviewId) {
    throw new Error('reviewId is required');
  }

  return request<{ success?: boolean; status?: string }>(
    `/task-manager/reviews/${encodeURIComponent(reviewId)}/decision`,
    {
    method: 'POST',
    body: JSON.stringify(payload),
    },
  );
};

export const getImConversationActionRequests = async (
  request: ApiRequestFn,
  conversationId: string,
): Promise<ImConversationActionRequestResponse[]> => {
  if (!conversationId) {
    return [];
  }

  const result = await request<
    { action_requests?: ImConversationActionRequestResponse[] } | ImConversationActionRequestResponse[]
  >(
    `/im/conversations/${encodeURIComponent(conversationId)}/action-requests`,
  );
  if (Array.isArray(result)) {
    return result;
  }
  return Array.isArray(result?.action_requests) ? result.action_requests : [];
};

export const getImConversations = async (
  request: ApiRequestFn,
): Promise<ImConversationResponse[]> => {
  const result = await request<ImConversationResponse[]>('/im/conversations');
  return Array.isArray(result) ? result : [];
};

export const getImWsMeta = async (
  request: ApiRequestFn,
): Promise<{ ws_url?: string | null }> => {
  const result = await request<{ ws_url?: string | null }>('/im/ws-meta');
  return result && typeof result === 'object' ? result : {};
};

export const createImConversation = (
  request: ApiRequestFn,
  payload: ImConversationCreatePayload,
): Promise<ImConversationResponse> => (
  request<ImConversationResponse>('/im/conversations', {
    method: 'POST',
    body: JSON.stringify(payload),
  })
);

export const markImConversationRead = (
  request: ApiRequestFn,
  conversationId: string,
): Promise<ImConversationResponse> => {
  if (!conversationId) {
    throw new Error('conversationId is required');
  }

  return request<ImConversationResponse>(
    `/im/conversations/${encodeURIComponent(conversationId)}/read`,
    {
      method: 'POST',
      body: JSON.stringify({}),
    },
  );
};

export const getImConversationMessages = async (
  request: ApiRequestFn,
  conversationId: string,
  options?: { limit?: number; order?: 'asc' | 'desc' },
): Promise<ImConversationMessageResponse[]> => {
  if (!conversationId) {
    return [];
  }

  const params = new URLSearchParams();
  if (typeof options?.limit === 'number') {
    params.set('limit', String(options.limit));
  }
  if (options?.order) {
    params.set('order', options.order);
  }

  const query = params.toString();
  const result = await request<ImConversationMessageResponse[]>(
    `/im/conversations/${encodeURIComponent(conversationId)}/messages${query ? `?${query}` : ''}`,
  );
  return Array.isArray(result) ? result : [];
};

export const createImConversationMessage = (
  request: ApiRequestFn,
  conversationId: string,
  payload: ImConversationMessageCreatePayload,
): Promise<ImConversationMessageResponse> => {
  if (!conversationId) {
    throw new Error('conversationId is required');
  }

  return request<ImConversationMessageResponse>(
    `/im/conversations/${encodeURIComponent(conversationId)}/messages`,
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
  );
};

export const getImConversationRuns = async (
  request: ApiRequestFn,
  conversationId: string,
): Promise<ImConversationRunResponse[]> => {
  if (!conversationId) {
    return [];
  }

  const result = await request<ImConversationRunResponse[]>(
    `/im/conversations/${encodeURIComponent(conversationId)}/runs`,
  );
  return Array.isArray(result) ? result : [];
};

export const submitImActionRequest = (
  request: ApiRequestFn,
  actionRequestId: string,
  payload: unknown,
): Promise<ImActionRequestSubmitResponse> => {
  if (!actionRequestId) {
    throw new Error('actionRequestId is required');
  }

  return request<ImActionRequestSubmitResponse>(
    `/im/action-requests/${encodeURIComponent(actionRequestId)}/submit`,
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
  );
};

export const getPendingUiPrompts = async (
  request: ApiRequestFn,
  sessionId: string,
  options?: { limit?: number }
): Promise<UiPromptItemResponse[]> => {
  if (!sessionId) {
    return [];
  }

  const params = new URLSearchParams();
  params.set('session_id', sessionId);
  if (typeof options?.limit === 'number') {
    params.set('limit', String(options.limit));
  }

  const result = await request<{ prompts?: UiPromptItemResponse[] } | UiPromptItemResponse[]>(
    '/ui-prompts/pending?' + params.toString(),
  );
  if (Array.isArray(result)) {
    return result;
  }
  return Array.isArray(result?.prompts) ? result.prompts : [];
};

export const getUiPromptHistory = async (
  request: ApiRequestFn,
  sessionId: string,
  options?: { limit?: number; includePending?: boolean }
): Promise<UiPromptItemResponse[]> => {
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

  const result = await request<{ prompts?: UiPromptItemResponse[] } | UiPromptItemResponse[]>(
    '/ui-prompts/history?' + params.toString(),
  );
  if (Array.isArray(result)) {
    return result;
  }
  return Array.isArray(result?.prompts) ? result.prompts : [];
};

export const submitUiPromptResponse = (
  request: ApiRequestFn,
  promptId: string,
  payload: UiPromptResponsePayload,
): Promise<{ success?: boolean; status?: string }> => {
  if (!promptId) {
    throw new Error('promptId is required');
  }

  return request<{ success?: boolean; status?: string }>(
    `/ui-prompts/${encodeURIComponent(promptId)}/respond`,
    {
    method: 'POST',
    body: JSON.stringify(payload),
    },
  );
};
