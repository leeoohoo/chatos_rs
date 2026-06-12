import type {
  MessageCreatePayload,
  MessageTaskRunnerRunDetailResponse,
  MessageTaskRunnerTask,
  MessageTaskRunnerTasksResponse,
  SessionMessageResponse,
} from './types';
import { buildQuery } from './shared';
import type { ApiRequestFn } from './workspace';

export interface MessageTaskRunnerLookupOptions {
  sessionId?: string | null;
  turnId?: string | null;
  sourceUserMessageId?: string | null;
}

const messageTaskRunnerLookupQuery = (
  options?: MessageTaskRunnerLookupOptions,
): string => buildQuery({
  session_id: options?.sessionId || undefined,
  turn_id: options?.turnId || undefined,
  source_user_message_id: options?.sourceUserMessageId || undefined,
});

export const createMessage = (
  request: ApiRequestFn,
  data: MessageCreatePayload,
): Promise<SessionMessageResponse> => {
  const requestData = {
    ...data,
    createdAt: data.createdAt ? data.createdAt.toISOString() : undefined,
  };
  return request<SessionMessageResponse>(`/conversations/${data.conversationId}/messages`, {
    method: 'POST',
    body: JSON.stringify(requestData),
  });
};

export const getMessageTaskRunnerTasks = (
  request: ApiRequestFn,
  messageId: string,
  options?: MessageTaskRunnerLookupOptions,
): Promise<MessageTaskRunnerTasksResponse> => {
  return request<MessageTaskRunnerTasksResponse>(
    `/messages/${encodeURIComponent(messageId)}/task-runner/tasks${messageTaskRunnerLookupQuery(options)}`,
  );
};

export const getMessageTaskRunnerTask = (
  request: ApiRequestFn,
  messageId: string,
  taskId: string,
  options?: MessageTaskRunnerLookupOptions,
): Promise<MessageTaskRunnerTask> => {
  return request<MessageTaskRunnerTask>(
    `/messages/${encodeURIComponent(messageId)}/task-runner/tasks/${encodeURIComponent(taskId)}${messageTaskRunnerLookupQuery(options)}`,
  );
};

export const getMessageTaskRunnerRun = (
  request: ApiRequestFn,
  messageId: string,
  runId: string,
  options?: MessageTaskRunnerLookupOptions,
): Promise<MessageTaskRunnerRunDetailResponse> => {
  return request<MessageTaskRunnerRunDetailResponse>(
    `/messages/${encodeURIComponent(messageId)}/task-runner/runs/${encodeURIComponent(runId)}${messageTaskRunnerLookupQuery(options)}`,
  );
};
