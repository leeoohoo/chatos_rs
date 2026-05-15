import { debugLog } from '@/lib/utils';
import type {
  TaskReviewItemResponse,
  TaskManagerTaskResponse,
  TaskManagerUpdatePayload,
  TaskReviewDecisionPayload,
  UiPromptItemResponse,
  UiPromptResponsePayload,
} from './types';
import type { ApiRequestFn } from './workspace';

const normalizeConversationId = (conversationId: string): string => (
  typeof conversationId === 'string' ? conversationId.trim() : ''
);

export const getTaskManagerTasks = async (
  request: ApiRequestFn,
  conversationId: string,
  options?: { conversationTurnId?: string; includeDone?: boolean; limit?: number }
): Promise<TaskManagerTaskResponse[]> => {
  const normalizedConversationId = normalizeConversationId(conversationId);
  if (!normalizedConversationId) {
    return [];
  }

  const params = new URLSearchParams();
  params.set('conversation_id', normalizedConversationId);
  if (options?.conversationTurnId) {
    params.set('conversation_turn_id', options.conversationTurnId);
  }
  if (options?.includeDone === true) {
    params.set('include_done', 'true');
  }
  if (typeof options?.limit === 'number') {
    params.set('limit', String(options.limit));
  }
  try {
    const result = await request<{ tasks?: TaskManagerTaskResponse[] } | TaskManagerTaskResponse[]>(
      '/task-manager/tasks?' + params.toString(),
    );
    if (Array.isArray(result)) {
      return result;
    }
    return Array.isArray(result?.tasks) ? result.tasks : [];
  } catch (error) {
    debugLog('[tasks] getTaskManagerTasks failed', {
      conversationId: normalizedConversationId,
      conversationTurnId: options?.conversationTurnId || null,
      includeDone: options?.includeDone === true,
      limit: options?.limit ?? null,
      error: error instanceof Error ? error.message : String(error),
    });
    throw error;
  }
};

export const updateTaskManagerTask = (
  request: ApiRequestFn,
  conversationId: string,
  taskId: string,
  payload: TaskManagerUpdatePayload,
): Promise<TaskManagerTaskResponse> => {
  const normalizedConversationId = normalizeConversationId(conversationId);
  if (!normalizedConversationId) {
    throw new Error('conversationId is required');
  }
  if (!taskId) {
    throw new Error('taskId is required');
  }

  const params = new URLSearchParams();
  params.set('conversation_id', normalizedConversationId);
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
  const normalizedConversationId = normalizeConversationId(conversationId);
  if (!normalizedConversationId) {
    throw new Error('conversationId is required');
  }
  if (!taskId) {
    throw new Error('taskId is required');
  }

  const params = new URLSearchParams();
  params.set('conversation_id', normalizedConversationId);
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
  const normalizedConversationId = normalizeConversationId(conversationId);
  if (!normalizedConversationId) {
    throw new Error('conversationId is required');
  }
  if (!taskId) {
    throw new Error('taskId is required');
  }

  const params = new URLSearchParams();
  params.set('conversation_id', normalizedConversationId);
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

export const getPendingTaskReviews = async (
  request: ApiRequestFn,
  conversationId: string,
  options?: { limit?: number }
): Promise<TaskReviewItemResponse[]> => {
  const normalizedConversationId = normalizeConversationId(conversationId);
  if (!normalizedConversationId) {
    return [];
  }

  const params = new URLSearchParams();
  params.set('conversation_id', normalizedConversationId);
  if (typeof options?.limit === 'number') {
    params.set('limit', String(options.limit));
  }

  try {
    const result = await request<{ reviews?: TaskReviewItemResponse[] } | TaskReviewItemResponse[]>(
      '/task-manager/reviews/pending?' + params.toString(),
    );
    if (Array.isArray(result)) {
      return result;
    }
    return Array.isArray(result?.reviews) ? result.reviews : [];
  } catch (error) {
    debugLog('[tasks] getPendingTaskReviews failed', {
      conversationId: normalizedConversationId,
      limit: options?.limit ?? null,
      error: error instanceof Error ? error.message : String(error),
    });
    throw error;
  }
};

export const getPendingUiPrompts = async (
  request: ApiRequestFn,
  conversationId: string,
  options?: { limit?: number }
): Promise<UiPromptItemResponse[]> => {
  const normalizedConversationId = normalizeConversationId(conversationId);
  if (!normalizedConversationId) {
    return [];
  }

  const params = new URLSearchParams();
  params.set('conversation_id', normalizedConversationId);
  if (typeof options?.limit === 'number') {
    params.set('limit', String(options.limit));
  }

  try {
    const result = await request<{ prompts?: UiPromptItemResponse[] } | UiPromptItemResponse[]>(
      '/ui-prompts/pending?' + params.toString(),
    );
    if (Array.isArray(result)) {
      return result;
    }
    return Array.isArray(result?.prompts) ? result.prompts : [];
  } catch (error) {
    debugLog('[tasks] getPendingUiPrompts failed', {
      conversationId: normalizedConversationId,
      limit: options?.limit ?? null,
      error: error instanceof Error ? error.message : String(error),
    });
    throw error;
  }
};

export const getUiPromptHistory = async (
  request: ApiRequestFn,
  conversationId: string,
  options?: { limit?: number; includePending?: boolean }
): Promise<UiPromptItemResponse[]> => {
  const normalizedConversationId = normalizeConversationId(conversationId);
  if (!normalizedConversationId) {
    return [];
  }

  const params = new URLSearchParams();
  params.set('conversation_id', normalizedConversationId);
  if (typeof options?.limit === 'number') {
    params.set('limit', String(options.limit));
  }
  if (options?.includePending === true) {
    params.set('include_pending', 'true');
  }

  try {
    const result = await request<{ prompts?: UiPromptItemResponse[] } | UiPromptItemResponse[]>(
      '/ui-prompts/history?' + params.toString(),
    );
    if (Array.isArray(result)) {
      return result;
    }
    return Array.isArray(result?.prompts) ? result.prompts : [];
  } catch (error) {
    debugLog('[tasks] getUiPromptHistory failed', {
      conversationId: normalizedConversationId,
      includePending: options?.includePending === true,
      limit: options?.limit ?? null,
      error: error instanceof Error ? error.message : String(error),
    });
    throw error;
  }
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
