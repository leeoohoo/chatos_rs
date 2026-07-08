// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  MessageTaskRunnerGraphResponse,
  MessageCreatePayload,
  MessageTaskRunnerRunDetailResponse,
  MessageTaskRunnerRunOutputChangesResponse,
  MessageTaskRunnerRunOutputDiffResponse,
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
  eventLimit?: number | null;
  eventOffset?: number | null;
  limit?: number | null;
  offset?: number | null;
  path?: string | null;
}

const messageTaskRunnerLookupQuery = (
  options?: MessageTaskRunnerLookupOptions,
): string => buildQuery({
  session_id: options?.sessionId || undefined,
  turn_id: options?.turnId || undefined,
  source_user_message_id: options?.sourceUserMessageId || undefined,
  event_limit: typeof options?.eventLimit === 'number' ? options.eventLimit : undefined,
  event_offset: typeof options?.eventOffset === 'number' ? options.eventOffset : undefined,
  limit: typeof options?.limit === 'number' ? options.limit : undefined,
  offset: typeof options?.offset === 'number' ? options.offset : undefined,
  path: options?.path || undefined,
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

export const getMessageTaskRunnerGraph = (
  request: ApiRequestFn,
  messageId: string,
  options?: MessageTaskRunnerLookupOptions,
): Promise<MessageTaskRunnerGraphResponse> => {
  return request<MessageTaskRunnerGraphResponse>(
    `/messages/${encodeURIComponent(messageId)}/task-runner/graph${messageTaskRunnerLookupQuery(options)}`,
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

export const getMessageTaskRunnerGraphRun = (
  request: ApiRequestFn,
  messageId: string,
  runId: string,
  options?: MessageTaskRunnerLookupOptions,
): Promise<MessageTaskRunnerRunDetailResponse> => {
  return request<MessageTaskRunnerRunDetailResponse>(
    `/messages/${encodeURIComponent(messageId)}/task-runner/graph/runs/${encodeURIComponent(runId)}${messageTaskRunnerLookupQuery(options)}`,
  );
};

export const getMessageTaskRunnerRunOutputChanges = (
  request: ApiRequestFn,
  messageId: string,
  runId: string,
  options?: MessageTaskRunnerLookupOptions,
): Promise<MessageTaskRunnerRunOutputChangesResponse> => {
  return request<MessageTaskRunnerRunOutputChangesResponse>(
    `/messages/${encodeURIComponent(messageId)}/task-runner/runs/${encodeURIComponent(runId)}/output/changes${messageTaskRunnerLookupQuery(options)}`,
  );
};

export const getMessageTaskRunnerRunOutputDiff = (
  request: ApiRequestFn,
  messageId: string,
  runId: string,
  path: string,
  options?: MessageTaskRunnerLookupOptions,
): Promise<MessageTaskRunnerRunOutputDiffResponse> => {
  return request<MessageTaskRunnerRunOutputDiffResponse>(
    `/messages/${encodeURIComponent(messageId)}/task-runner/runs/${encodeURIComponent(runId)}/output/diff${messageTaskRunnerLookupQuery({ ...options, path })}`,
  );
};
