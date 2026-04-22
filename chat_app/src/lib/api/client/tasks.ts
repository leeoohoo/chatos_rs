import type {
  TaskManagerTaskResponse,
  TaskManagerUpdatePayload,
  TaskReviewDecisionPayload,
  UiPromptItemResponse,
  UiPromptResponsePayload,
} from './types';
import type { ApiRequestFn } from './workspace';

export const getTaskManagerTasks = async (
  request: ApiRequestFn,
  conversationId: string,
  options?: { conversationTurnId?: string; includeDone?: boolean; limit?: number }
): Promise<TaskManagerTaskResponse[]> => {
  if (!conversationId) {
    return [];
  }

  const params = new URLSearchParams();
  params.set('conversation_id', conversationId);
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
  conversationId: string,
  taskId: string,
  payload: TaskManagerUpdatePayload,
): Promise<TaskManagerTaskResponse> => {
  if (!conversationId) {
    throw new Error('conversationId is required');
  }
  if (!taskId) {
    throw new Error('taskId is required');
  }

  const params = new URLSearchParams();
  params.set('conversation_id', conversationId);
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
  conversationId: string,
  taskId: string,
  payload?: Partial<TaskManagerUpdatePayload>
): Promise<TaskManagerTaskResponse> => {
  if (!conversationId) {
    throw new Error('conversationId is required');
  }
  if (!taskId) {
    throw new Error('taskId is required');
  }

  const params = new URLSearchParams();
  params.set('conversation_id', conversationId);
  return request<TaskManagerTaskResponse>(
    '/task-manager/tasks/' + encodeURIComponent(taskId) + '/complete?' + params.toString(),
    {
    method: 'POST',
    body: JSON.stringify(payload || {}),
    },
  );
};

export const deleteTaskManagerTask = (
  request: ApiRequestFn,
  conversationId: string,
  taskId: string
): Promise<{ success?: boolean }> => {
  if (!conversationId) {
    throw new Error('conversationId is required');
  }
  if (!taskId) {
    throw new Error('taskId is required');
  }

  const params = new URLSearchParams();
  params.set('conversation_id', conversationId);
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

export const getPendingUiPrompts = async (
  request: ApiRequestFn,
  conversationId: string,
  options?: { limit?: number }
): Promise<UiPromptItemResponse[]> => {
  if (!conversationId) {
    return [];
  }

  const params = new URLSearchParams();
  params.set('conversation_id', conversationId);
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
  conversationId: string,
  options?: { limit?: number; includePending?: boolean }
): Promise<UiPromptItemResponse[]> => {
  if (!conversationId) {
    return [];
  }

  const params = new URLSearchParams();
  params.set('conversation_id', conversationId);
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
