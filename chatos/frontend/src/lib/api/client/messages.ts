// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  MessageTaskRunnerGraphResponse,
  MessageCreatePayload,
  MessageTaskRunnerRunDetailResponse,
  MessageTaskRunnerRunChanges,
  MessageTaskRunnerRetryRunResponse,
  MessageTaskRunnerTask,
  MessageTaskRunnerTasksResponse,
  PluginUiWorkbenchSessionResponse,
  PluginArtifactListResponse,
  PluginArtifactCreateRequest,
  PluginArtifactReadResponse,
  PluginArtifactUpdateRequest,
  PluginArtifactWriteResponse,
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
  includeEvents?: boolean | null;
}

const messageTaskRunnerLookupQuery = (
  options?: MessageTaskRunnerLookupOptions,
): string => buildQuery({
  session_id: options?.sessionId || undefined,
  turn_id: options?.turnId || undefined,
  source_user_message_id: options?.sourceUserMessageId || undefined,
  event_limit: typeof options?.eventLimit === 'number' ? options.eventLimit : undefined,
  event_offset: typeof options?.eventOffset === 'number' ? options.eventOffset : undefined,
  include_events: typeof options?.includeEvents === 'boolean' ? options.includeEvents : undefined,
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

export const getMessageTaskRunnerRunChanges = (
  request: ApiRequestFn,
  messageId: string,
  runId: string,
  options?: MessageTaskRunnerLookupOptions,
): Promise<MessageTaskRunnerRunChanges> => request<MessageTaskRunnerRunChanges>(
  `/messages/${encodeURIComponent(messageId)}/task-runner/runs/${encodeURIComponent(runId)}/changes${messageTaskRunnerLookupQuery(options)}`,
);

export const retryMessageTaskRunnerRun = (
  request: ApiRequestFn,
  messageId: string,
  runId: string,
  options?: MessageTaskRunnerLookupOptions,
  retryInstruction?: string | null,
  executionServiceId?: string | null,
): Promise<MessageTaskRunnerRetryRunResponse> => {
  const normalizedInstruction = retryInstruction?.trim();
  const normalizedExecutionServiceId = executionServiceId?.trim();
  return request<MessageTaskRunnerRetryRunResponse>(
    `/messages/${encodeURIComponent(messageId)}/task-runner/runs/${encodeURIComponent(runId)}/retry${messageTaskRunnerLookupQuery(options)}`,
    {
      method: 'POST',
      body: JSON.stringify({
        ...(normalizedInstruction ? { retry_instruction: normalizedInstruction } : {}),
        ...(normalizedExecutionServiceId
          ? { execution_service_id: normalizedExecutionServiceId }
          : {}),
      }),
    },
  );
};

export const retryMessageTaskRunnerRunIntegration = (
  request: ApiRequestFn,
  messageId: string,
  runId: string,
  options?: MessageTaskRunnerLookupOptions,
): Promise<MessageTaskRunnerRetryRunResponse> => request<MessageTaskRunnerRetryRunResponse>(
  `/messages/${encodeURIComponent(messageId)}/task-runner/runs/${encodeURIComponent(runId)}/integration/retry${messageTaskRunnerLookupQuery(options)}`,
  { method: 'POST' },
);

export const waiveMessageTaskRunnerRunIntegration = (
  request: ApiRequestFn,
  messageId: string,
  runId: string,
  reason: string,
  options?: MessageTaskRunnerLookupOptions,
): Promise<MessageTaskRunnerRetryRunResponse> => request<MessageTaskRunnerRetryRunResponse>(
  `/messages/${encodeURIComponent(messageId)}/task-runner/runs/${encodeURIComponent(runId)}/integration/waive${messageTaskRunnerLookupQuery(options)}`,
  {
    method: 'POST',
    body: JSON.stringify({ reason: reason.trim() }),
  },
);

export const createPluginUiWorkbenchSession = (
  request: ApiRequestFn,
  messageId: string,
  runId: string,
  eventId: string,
  options?: MessageTaskRunnerLookupOptions,
): Promise<PluginUiWorkbenchSessionResponse> => request<PluginUiWorkbenchSessionResponse>(
  `/messages/${encodeURIComponent(messageId)}/task-runner/runs/${encodeURIComponent(runId)}/plugin-ui/${encodeURIComponent(eventId)}/workbench-sessions${messageTaskRunnerLookupQuery(options)}`,
  { method: 'POST' },
);

export const revokePluginUiWorkbenchSession = (
  request: ApiRequestFn,
  messageId: string,
  runId: string,
  eventId: string,
  sessionId: string,
): Promise<void> => request<void>(
  `/messages/${encodeURIComponent(messageId)}/task-runner/runs/${encodeURIComponent(runId)}/plugin-ui/${encodeURIComponent(eventId)}/workbench-sessions/${encodeURIComponent(sessionId)}`,
  { method: 'DELETE' },
);

export const listPluginUiWorkbenchArtifacts = (
  request: ApiRequestFn,
  messageId: string,
  runId: string,
  eventId: string,
  sessionId: string,
): Promise<PluginArtifactListResponse> => request<PluginArtifactListResponse>(
  `/messages/${encodeURIComponent(messageId)}/task-runner/runs/${encodeURIComponent(runId)}/plugin-ui/${encodeURIComponent(eventId)}/workbench-sessions/${encodeURIComponent(sessionId)}/artifacts`,
);

export const readPluginUiWorkbenchArtifact = (
  request: ApiRequestFn,
  messageId: string,
  runId: string,
  eventId: string,
  sessionId: string,
  artifactId: string,
): Promise<PluginArtifactReadResponse> => request<PluginArtifactReadResponse>(
  `/messages/${encodeURIComponent(messageId)}/task-runner/runs/${encodeURIComponent(runId)}/plugin-ui/${encodeURIComponent(eventId)}/workbench-sessions/${encodeURIComponent(sessionId)}/artifacts/${encodeURIComponent(artifactId)}`,
);

export const createPluginUiWorkbenchArtifact = (
  request: ApiRequestFn,
  messageId: string,
  runId: string,
  eventId: string,
  sessionId: string,
  payload: PluginArtifactCreateRequest,
): Promise<PluginArtifactWriteResponse> => request<PluginArtifactWriteResponse>(
  `/messages/${encodeURIComponent(messageId)}/task-runner/runs/${encodeURIComponent(runId)}/plugin-ui/${encodeURIComponent(eventId)}/workbench-sessions/${encodeURIComponent(sessionId)}/artifacts`,
  {
    method: 'POST',
    body: JSON.stringify(payload),
  },
);

export const updatePluginUiWorkbenchArtifact = (
  request: ApiRequestFn,
  messageId: string,
  runId: string,
  eventId: string,
  sessionId: string,
  artifactId: string,
  payload: PluginArtifactUpdateRequest,
): Promise<PluginArtifactWriteResponse> => request<PluginArtifactWriteResponse>(
  `/messages/${encodeURIComponent(messageId)}/task-runner/runs/${encodeURIComponent(runId)}/plugin-ui/${encodeURIComponent(eventId)}/workbench-sessions/${encodeURIComponent(sessionId)}/artifacts/${encodeURIComponent(artifactId)}`,
  {
    method: 'PUT',
    body: JSON.stringify(payload),
  },
);

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
