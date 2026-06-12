import type {
  MessageCreatePayload,
  MessageTaskRunnerRunDetailResponse,
  MessageTaskRunnerTask,
  MessageTaskRunnerTasksResponse,
  SessionMessageResponse,
} from './types';
import type { ApiRequestFn } from './workspace';

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
): Promise<MessageTaskRunnerTasksResponse> => {
  return request<MessageTaskRunnerTasksResponse>(
    `/messages/${encodeURIComponent(messageId)}/task-runner/tasks`,
  );
};

export const getMessageTaskRunnerTask = (
  request: ApiRequestFn,
  messageId: string,
  taskId: string,
): Promise<MessageTaskRunnerTask> => {
  return request<MessageTaskRunnerTask>(
    `/messages/${encodeURIComponent(messageId)}/task-runner/tasks/${encodeURIComponent(taskId)}`,
  );
};

export const getMessageTaskRunnerRun = (
  request: ApiRequestFn,
  messageId: string,
  runId: string,
): Promise<MessageTaskRunnerRunDetailResponse> => {
  return request<MessageTaskRunnerRunDetailResponse>(
    `/messages/${encodeURIComponent(messageId)}/task-runner/runs/${encodeURIComponent(runId)}`,
  );
};
