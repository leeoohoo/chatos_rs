// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  AgentToolsResponse,
  AskUserPromptListResponse,
  AskUserPromptMutationPayload,
  AskUserPromptMutationResponse,
  ConversationTaskRunnerActiveMessageTasksResponse,
  ReviewRepairResponse,
  ReviewRepairStatusResponse,
  SessionMessageResponse,
  SessionResponse,
  SessionRuntimeSettingsPayload,
  SessionRuntimeSettingsResponse,
  SessionSummariesListResponse,
  SessionUpsertPayload,
  TaskManagerTaskResponse,
  TaskManagerUpdatePayload,
  UserMessageTurnsResponse,
} from '../client/types';
import {
  askUserSessionId,
  cancelLocalAskUserPrompt,
  listLocalAskUserPrompts,
  submitLocalAskUserPrompt,
} from './askUserPrompts';
import { requestLocalRuntime } from './bridge';
import { LocalRuntimeProjectClient } from './projectClient';
import { readLocalSessionSelection } from './sessionMetadata';
import {
  completeLocalTaskManagerTask,
  deleteLocalTaskManagerTask,
  getLocalActiveMessageTasks,
  getLocalTaskBoardTasks,
  getLocalTaskManagerTasks,
  updateLocalTaskManagerTask,
} from './taskBoard';
import type { LocalRuntimeEventRecord, LocalRuntimeSessionRecord } from './types';
import { buildLocalUserMessageTurns } from './userMessageTurns';

const toSessionResponse = (
  record: LocalRuntimeSessionRecord,
  metadata: Record<string, unknown> = {},
): SessionResponse => ({
  id: record.id,
  title: record.title,
  user_id: record.owner_user_id,
  project_id: record.project_id,
  selected_model_id: record.selected_model_id ?? null,
  selected_agent_id: record.selected_agent_id ?? null,
  status: record.status,
  message_count: record.message_count,
  created_at: record.created_at,
  updated_at: record.updated_at,
  metadata: {
    ...metadata,
    runtime_origin: 'local_device',
  },
});

export class LocalRuntimeSessionClient extends LocalRuntimeProjectClient {
  async getSessions(projectId: string): Promise<SessionResponse[]> {
    const records = await requestLocalRuntime<LocalRuntimeSessionRecord[]>(
      `/api/local/runtime/sessions?project_id=${encodeURIComponent(projectId)}`,
    );
    return records.map((record) => toSessionResponse(record));
  }

  async createSession(data: SessionUpsertPayload): Promise<SessionResponse> {
    const selection = readLocalSessionSelection(data.metadata);
    const record = await requestLocalRuntime<LocalRuntimeSessionRecord>(
      '/api/local/runtime/sessions',
      {
        method: 'POST',
        body: JSON.stringify({
          project_id: data.project_id,
          title: data.title,
          selected_model_id: selection.selectedModelId,
          selected_agent_id: selection.selectedAgentId,
        }),
      },
    );
    return toSessionResponse(record, selection.metadata);
  }

  async getSession(sessionId: string): Promise<SessionResponse> {
    const record = await requestLocalRuntime<LocalRuntimeSessionRecord>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}`,
    );
    return toSessionResponse(record);
  }

  async getMessages(sessionId: string): Promise<SessionMessageResponse[]> {
    return requestLocalRuntime<SessionMessageResponse[]>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/messages`,
    );
  }

  async getUserMessageTurns(
    sessionId: string,
    options: { limit?: number; before?: string | null } = {},
  ): Promise<UserMessageTurnsResponse> {
    const [messages, taskResponse] = await Promise.all([
      this.getMessages(sessionId),
      getLocalTaskBoardTasks(sessionId, { includeDone: true, limit: 200 }),
    ]);
    return buildLocalUserMessageTurns(messages, taskResponse.items || [], options);
  }

  async getActiveMessageTasks(
    sessionId: string,
  ): Promise<ConversationTaskRunnerActiveMessageTasksResponse> {
    return getLocalActiveMessageTasks(sessionId);
  }

  async getTaskManagerTasks(
    sessionId: string,
    options: { conversationTurnId?: string; includeDone?: boolean; limit?: number } = {},
  ): Promise<TaskManagerTaskResponse[]> {
    return getLocalTaskManagerTasks(sessionId, options);
  }

  async updateTaskManagerTask(
    sessionId: string,
    taskId: string,
    payload: TaskManagerUpdatePayload,
  ): Promise<TaskManagerTaskResponse> {
    return updateLocalTaskManagerTask(sessionId, taskId, payload);
  }

  async completeTaskManagerTask(
    sessionId: string,
    taskId: string,
    payload: Partial<TaskManagerUpdatePayload> = {},
  ): Promise<TaskManagerTaskResponse> {
    return completeLocalTaskManagerTask(sessionId, taskId, payload);
  }

  async deleteTaskManagerTask(
    sessionId: string,
    taskId: string,
  ): Promise<{ success?: boolean }> {
    return deleteLocalTaskManagerTask(sessionId, taskId);
  }

  async getRuntimeEvents(
    sessionId: string,
    options: { turnId?: string | null; after?: number; limit?: number } = {},
  ): Promise<LocalRuntimeEventRecord[]> {
    const query = new URLSearchParams();
    const turnId = String(options.turnId || '').trim();
    if (turnId) {
      query.set('turn_id', turnId);
    }
    if (Number.isFinite(options.after)) {
      query.set('after', String(Math.max(0, Math.trunc(options.after || 0))));
    }
    if (Number.isFinite(options.limit)) {
      query.set('limit', String(Math.max(1, Math.trunc(options.limit || 100))));
    }
    const suffix = query.size > 0 ? `?${query.toString()}` : '';
    return requestLocalRuntime<LocalRuntimeEventRecord[]>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/events${suffix}`,
    );
  }

  async getConversationSummaries(
    sessionId: string,
    options: { limit?: number; offset?: number } = {},
  ): Promise<SessionSummariesListResponse> {
    const query = new URLSearchParams();
    if (Number.isFinite(options.limit)) {
      query.set('limit', String(Math.max(1, Math.trunc(options.limit || 100))));
    }
    if (Number.isFinite(options.offset)) {
      query.set('offset', String(Math.max(0, Math.trunc(options.offset || 0))));
    }
    const suffix = query.size > 0 ? `?${query.toString()}` : '';
    return requestLocalRuntime<SessionSummariesListResponse>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/summaries${suffix}`,
    );
  }

  async deleteConversationSummary(
    sessionId: string,
    summaryId: string,
  ): Promise<{ success?: boolean }> {
    return requestLocalRuntime<{ success?: boolean }>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/summaries/${encodeURIComponent(summaryId)}`,
      { method: 'DELETE' },
    );
  }

  async clearConversationSummaries(sessionId: string): Promise<{ success?: boolean }> {
    return requestLocalRuntime<{ success?: boolean }>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/summaries`,
      { method: 'DELETE' },
    );
  }

  async runConversationReviewRepair(sessionId: string): Promise<ReviewRepairResponse> {
    return requestLocalRuntime<ReviewRepairResponse>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/review-repair`,
      { method: 'POST' },
    );
  }

  async getConversationReviewRepairStatus(
    sessionId: string,
  ): Promise<ReviewRepairStatusResponse> {
    return requestLocalRuntime<ReviewRepairStatusResponse>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/review-repair`,
    );
  }

  async getConversationMemoryRecalls(
    sessionId: string,
    options: { limit?: number } = {},
  ): Promise<unknown[]> {
    const query = new URLSearchParams();
    if (Number.isFinite(options.limit)) {
      query.set('limit', String(Math.max(1, Math.trunc(options.limit || 20))));
    }
    const suffix = query.size > 0 ? `?${query.toString()}` : '';
    return requestLocalRuntime<unknown[]>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/memory-recalls${suffix}`,
    );
  }

  async deleteConversationMemoryRecall(
    sessionId: string,
    recallId: string,
  ): Promise<{ success?: boolean; deleted_recalls?: number }> {
    return requestLocalRuntime<{ success?: boolean; deleted_recalls?: number }>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/memory-recalls/${encodeURIComponent(recallId)}`,
      { method: 'DELETE' },
    );
  }

  async getRuntimeSettings(sessionId: string): Promise<SessionRuntimeSettingsResponse> {
    return requestLocalRuntime<SessionRuntimeSettingsResponse>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/runtime-settings`,
    );
  }

  async getAgentTools(sessionId: string): Promise<AgentToolsResponse> {
    return requestLocalRuntime<AgentToolsResponse>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/tools`,
    );
  }

  async listAskUserPrompts(
    sessionId: string,
    options: { includePending?: boolean; limit?: number } = {},
  ): Promise<AskUserPromptListResponse> {
    return listLocalAskUserPrompts(sessionId, options);
  }

  async submitAskUserPrompt(
    promptId: string,
    payload: AskUserPromptMutationPayload,
  ): Promise<AskUserPromptMutationResponse> {
    return submitLocalAskUserPrompt(askUserSessionId(payload), promptId, payload);
  }

  async cancelAskUserPrompt(
    promptId: string,
    payload: Pick<AskUserPromptMutationPayload, 'conversation_id' | 'conversationId' | 'reason'>,
  ): Promise<AskUserPromptMutationResponse> {
    return cancelLocalAskUserPrompt(askUserSessionId(payload), promptId, payload);
  }

  async updateRuntimeSettings(
    sessionId: string,
    data: SessionRuntimeSettingsPayload,
  ): Promise<SessionRuntimeSettingsResponse> {
    return requestLocalRuntime<SessionRuntimeSettingsResponse>(
      `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/runtime-settings`,
      {
        method: 'PUT',
        body: JSON.stringify(data),
      },
    );
  }
}
