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
    return workspaceApi.getSessions(this.getRequestFn(), userId, projectId, paging);
  },
  async createSession(data) {
    return workspaceApi.createSession(this.getRequestFn(), data);
  },
  async getSession(id) {
    return workspaceApi.getSession(this.getRequestFn(), id);
  },
  async updateSession(id, data) {
    return workspaceApi.updateSession(this.getRequestFn(), id, data);
  },
  async getConversationRuntimeSettings(conversationId) {
    return workspaceApi.getConversationRuntimeSettings(this.getRequestFn(), conversationId);
  },
  async updateConversationRuntimeSettings(conversationId, data) {
    return workspaceApi.updateConversationRuntimeSettings(this.getRequestFn(), conversationId, data);
  },
  async deleteSession(id) {
    return workspaceApi.deleteSession(this.getRequestFn(), id);
  },
  async getConversationMessages(conversationId, params) {
    return workspaceApi.getConversationMessages(this.getRequestFn(), conversationId, params);
  },
  async getConversationCompactHistory(conversationId, params) {
    return workspaceApi.getConversationCompactHistory(this.getRequestFn(), conversationId, params);
  },
  async getConversationUserMessageTurns(conversationId, params) {
    return workspaceApi.getConversationUserMessageTurns(this.getRequestFn(), conversationId, params);
  },
  async getConversationTaskRunnerActiveMessageTasks(conversationId, params) {
    return workspaceApi.getConversationTaskRunnerActiveMessageTasks(this.getRequestFn(), conversationId, params);
  },
  async getConversationTurnMessages(conversationId, userMessageId) {
    return workspaceApi.getConversationTurnMessages(this.getRequestFn(), conversationId, userMessageId);
  },
  async getConversationTurnMessagesByTurn(conversationId, turnId) {
    return workspaceApi.getConversationTurnMessagesByTurn(this.getRequestFn(), conversationId, turnId);
  },
  async getConversationLatestTurnRuntimeContext(conversationId) {
    return workspaceApi.getConversationLatestTurnRuntimeContext(this.getRequestFn(), conversationId);
  },
  async getConversationTurnRuntimeContextByTurn(conversationId, turnId) {
    return workspaceApi.getConversationTurnRuntimeContextByTurn(this.getRequestFn(), conversationId, turnId);
  },
  async createMessage(data) {
    return messagesApi.createMessage(this.getRequestFn(), data);
  },
};
