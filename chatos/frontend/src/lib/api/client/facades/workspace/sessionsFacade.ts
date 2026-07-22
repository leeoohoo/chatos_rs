// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import * as messagesApi from '../../messages';
import * as workspaceApi from '../../workspace';
import type {
  CompactHistoryResponse,
  ConversationTaskRunnerActiveMessageTasksResponse,
  DeleteSuccessResponse,
  MessageCreatePayload,
  SessionMessageResponse,
  SessionPagingOptions,
  SessionResponse,
  SessionRuntimeSettingsPayload,
  SessionRuntimeSettingsResponse,
  SessionUpdatePayload,
  SessionUpsertPayload,
  TurnRuntimeSnapshotLookupResponse,
  UserMessageTurnsResponse,
} from '../../types';
import type ApiClient from '../../../client';
import {
  assertCloudSessionOperation,
  isLocalRuntimeSessionId,
} from '../../../localRuntime';

export interface WorkspaceSessionFacade {
  getSessions(
    userId?: string,
    projectId?: string,
    paging?: SessionPagingOptions,
  ): Promise<SessionResponse[]>;
  createSession(data: SessionUpsertPayload): Promise<SessionResponse>;
  getSession(id: string): Promise<SessionResponse>;
  updateSession(id: string, data: SessionUpdatePayload): Promise<SessionResponse>;
  getConversationRuntimeSettings(conversationId: string): Promise<SessionRuntimeSettingsResponse>;
  updateConversationRuntimeSettings(
    conversationId: string,
    data: SessionRuntimeSettingsPayload,
  ): Promise<SessionRuntimeSettingsResponse>;
  deleteSession(id: string): Promise<DeleteSuccessResponse>;
  getConversationMessages(
    conversationId: string,
    params?: { limit?: number; offset?: number; compact?: boolean; strategy?: string },
  ): Promise<SessionMessageResponse[]>;
  getConversationCompactHistory(
    conversationId: string,
    params?: { limit?: number; before?: string | null },
  ): Promise<CompactHistoryResponse>;
  getConversationUserMessageTurns(
    conversationId: string,
    params?: { limit?: number; before?: string | null },
  ): Promise<UserMessageTurnsResponse>;
  getConversationTaskRunnerActiveMessageTasks(
    conversationId: string,
    params?: { sourceUserMessageIds?: string[]; sourceTurnIds?: string[] },
  ): Promise<ConversationTaskRunnerActiveMessageTasksResponse>;
  getConversationTurnMessages(conversationId: string, userMessageId: string): Promise<SessionMessageResponse[]>;
  getConversationTurnMessagesByTurn(conversationId: string, turnId: string): Promise<SessionMessageResponse[]>;
  getConversationLatestTurnRuntimeContext(conversationId: string): Promise<TurnRuntimeSnapshotLookupResponse>;
  getConversationTurnRuntimeContextByTurn(
    conversationId: string,
    turnId: string,
  ): Promise<TurnRuntimeSnapshotLookupResponse>;
  createMessage(data: MessageCreatePayload): Promise<SessionMessageResponse>;
}

export const workspaceSessionFacade: WorkspaceSessionFacade & ThisType<ApiClient> = {
  async getSessions(userId, projectId, paging) {
    if (this.sessionScopeUsesLocalRuntime(projectId)) {
      return this.getLocalRuntimeClient().getSessions(projectId || '-1');
    }
    return workspaceApi.getSessions(this.getRequestFn(), userId, projectId, paging);
  },
  async createSession(data) {
    if (this.sessionScopeUsesLocalRuntime(data.project_id)) {
      return this.getLocalRuntimeClient().createSession(data);
    }
    return workspaceApi.createSession(this.getRequestFn(), data);
  },
  async getSession(id) {
    if (isLocalRuntimeSessionId(id)) {
      return this.getLocalRuntimeClient().getSession(id);
    }
    return workspaceApi.getSession(this.getRequestFn(), id);
  },
  async updateSession(id, data) {
    assertCloudSessionOperation(id, '更新会话');
    return workspaceApi.updateSession(this.getRequestFn(), id, data);
  },
  async getConversationRuntimeSettings(conversationId) {
    if (isLocalRuntimeSessionId(conversationId)) {
      return this.getLocalRuntimeClient().getRuntimeSettings(conversationId);
    }
    return workspaceApi.getConversationRuntimeSettings(this.getRequestFn(), conversationId);
  },
  async updateConversationRuntimeSettings(conversationId, data) {
    if (isLocalRuntimeSessionId(conversationId)) {
      return this.getLocalRuntimeClient().updateRuntimeSettings(conversationId, data);
    }
    return workspaceApi.updateConversationRuntimeSettings(this.getRequestFn(), conversationId, data);
  },
  async deleteSession(id) {
    assertCloudSessionOperation(id, '删除会话');
    return workspaceApi.deleteSession(this.getRequestFn(), id);
  },
  async getConversationMessages(conversationId, params) {
    if (isLocalRuntimeSessionId(conversationId)) {
      return this.getLocalRuntimeClient().getMessages(conversationId);
    }
    return workspaceApi.getConversationMessages(this.getRequestFn(), conversationId, params);
  },
  async getConversationCompactHistory(conversationId, params) {
    if (isLocalRuntimeSessionId(conversationId)) {
      const items = await this.getLocalRuntimeClient().getMessages(conversationId);
      return { items, has_more: false, next_before: null };
    }
    return workspaceApi.getConversationCompactHistory(this.getRequestFn(), conversationId, params);
  },
  async getConversationUserMessageTurns(conversationId, params) {
    if (isLocalRuntimeSessionId(conversationId)) {
      return this.getLocalRuntimeClient().getUserMessageTurns(conversationId, params);
    }
    return workspaceApi.getConversationUserMessageTurns(this.getRequestFn(), conversationId, params);
  },
  async getConversationTaskRunnerActiveMessageTasks(conversationId, params) {
    if (isLocalRuntimeSessionId(conversationId)) {
      return this.getLocalRuntimeClient().getActiveMessageTasks(conversationId);
    }
    return workspaceApi.getConversationTaskRunnerActiveMessageTasks(this.getRequestFn(), conversationId, params);
  },
  async getConversationTurnMessages(conversationId, userMessageId) {
    if (isLocalRuntimeSessionId(conversationId)) {
      const messages = await this.getLocalRuntimeClient().getMessages(conversationId);
      const turnId = messages.find((message) => message.id === userMessageId)?.turn_id;
      return turnId ? messages.filter((message) => message.turn_id === turnId) : [];
    }
    return workspaceApi.getConversationTurnMessages(this.getRequestFn(), conversationId, userMessageId);
  },
  async getConversationTurnMessagesByTurn(conversationId, turnId) {
    if (isLocalRuntimeSessionId(conversationId)) {
      const messages = await this.getLocalRuntimeClient().getMessages(conversationId);
      return messages.filter((message) => message.turn_id === turnId);
    }
    return workspaceApi.getConversationTurnMessagesByTurn(this.getRequestFn(), conversationId, turnId);
  },
  async getConversationLatestTurnRuntimeContext(conversationId) {
    if (isLocalRuntimeSessionId(conversationId)) {
      return {
        conversation_id: conversationId,
        status: 'unavailable',
        snapshot_source: 'local_runtime',
        active_in_runtime: false,
        snapshot: null,
      };
    }
    return workspaceApi.getConversationLatestTurnRuntimeContext(this.getRequestFn(), conversationId);
  },
  async getConversationTurnRuntimeContextByTurn(conversationId, turnId) {
    if (isLocalRuntimeSessionId(conversationId)) {
      return {
        conversation_id: conversationId,
        turn_id: turnId,
        status: 'unavailable',
        snapshot_source: 'local_runtime',
        active_in_runtime: false,
        snapshot: null,
      };
    }
    return workspaceApi.getConversationTurnRuntimeContextByTurn(this.getRequestFn(), conversationId, turnId);
  },
  async createMessage(data) {
    assertCloudSessionOperation(data.conversationId, '直接创建消息');
    return messagesApi.createMessage(this.getRequestFn(), data);
  },
};
