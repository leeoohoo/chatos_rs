// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { debugLog } from '@/lib/utils';
import type {
  TaskManagerTaskResponse,
  TaskManagerUpdatePayload,
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
