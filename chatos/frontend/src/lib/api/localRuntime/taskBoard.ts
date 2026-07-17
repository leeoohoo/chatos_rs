// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { MessageTaskRunnerLookupOptions } from '../client/messages';
import type {
  ConversationTaskRunnerActiveMessageTasksResponse,
  MessageTaskRunnerGraphResponse,
  MessageTaskRunnerTask,
  MessageTaskRunnerTasksResponse,
  TaskManagerTaskResponse,
  TaskManagerUpdatePayload,
} from '../client/types';
import { requestLocalRuntime } from './bridge';

const taskBoardQuery = (
  options?: MessageTaskRunnerLookupOptions & { includeDone?: boolean },
): string => {
  const query = new URLSearchParams();
  if (options?.turnId) query.set('turn_id', options.turnId);
  if (options?.sourceUserMessageId) {
    query.set('source_user_message_id', options.sourceUserMessageId);
  }
  if (options?.includeDone !== undefined) {
    query.set('include_done', String(options.includeDone));
  }
  if (typeof options?.limit === 'number') query.set('limit', String(options.limit));
  return query.size > 0 ? `?${query.toString()}` : '';
};

export const getLocalTaskBoardTasks = (
  sessionId: string,
  options?: MessageTaskRunnerLookupOptions & { includeDone?: boolean },
): Promise<MessageTaskRunnerTasksResponse> => requestLocalRuntime<MessageTaskRunnerTasksResponse>(
  `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/task-board/tasks${taskBoardQuery(options)}`,
);

export const getLocalTaskBoardGraph = (
  sessionId: string,
  options?: MessageTaskRunnerLookupOptions,
): Promise<MessageTaskRunnerGraphResponse> => requestLocalRuntime<MessageTaskRunnerGraphResponse>(
  `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/task-board/graph${taskBoardQuery(options)}`,
);

export const getLocalTaskBoardTask = (
  sessionId: string,
  taskId: string,
): Promise<MessageTaskRunnerTask> => requestLocalRuntime<MessageTaskRunnerTask>(
  `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/task-board/tasks/${encodeURIComponent(taskId)}`,
);

export const getLocalActiveMessageTasks = (
  sessionId: string,
): Promise<ConversationTaskRunnerActiveMessageTasksResponse> => requestLocalRuntime<
  ConversationTaskRunnerActiveMessageTasksResponse
>(
  `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/task-board/active-message-tasks`,
);

export const getLocalTaskManagerTasks = async (
  sessionId: string,
  options: { conversationTurnId?: string; includeDone?: boolean; limit?: number } = {},
): Promise<TaskManagerTaskResponse[]> => {
  const response = await getLocalTaskBoardTasks(sessionId, {
    turnId: options.conversationTurnId,
    includeDone: options.includeDone,
    limit: options.limit,
  });
  return (response.items || []).map((task) => ({
    id: task.id,
    title: task.title,
    details: task.description ?? task.objective ?? null,
    priority: task.priority === 10 ? 'high' : task.priority === -10 ? 'low' : 'medium',
    status: task.status as TaskManagerTaskResponse['status'],
    tags: task.tags,
    due_at: typeof task.task_tool_state?.due_at === 'string'
      ? task.task_tool_state.due_at
      : null,
    outcome_summary: task.result_summary,
    resume_hint: task.process_log,
    conversation_turn_id: task.source_turn_id,
    created_at: task.created_at || undefined,
    updated_at: task.updated_at || undefined,
  }));
};

const localTaskPath = (sessionId: string, taskId: string, suffix = ''): string => (
  `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}`
  + `/task-board/tasks/${encodeURIComponent(taskId)}${suffix}`
);

export const updateLocalTaskManagerTask = (
  sessionId: string,
  taskId: string,
  payload: TaskManagerUpdatePayload,
): Promise<TaskManagerTaskResponse> => requestLocalRuntime(
  localTaskPath(sessionId, taskId),
  { method: 'PATCH', body: JSON.stringify(payload) },
);

export const completeLocalTaskManagerTask = (
  sessionId: string,
  taskId: string,
  payload: Partial<TaskManagerUpdatePayload> = {},
): Promise<TaskManagerTaskResponse> => requestLocalRuntime(
  localTaskPath(sessionId, taskId, '/complete'),
  { method: 'POST', body: JSON.stringify(payload) },
);

export const deleteLocalTaskManagerTask = (
  sessionId: string,
  taskId: string,
): Promise<{ success?: boolean }> => requestLocalRuntime(
  localTaskPath(sessionId, taskId),
  { method: 'DELETE' },
);
