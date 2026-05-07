import * as messagesApi from '../../messages';
import * as workspaceApi from '../../workspace';
import type {
  DeleteSuccessResponse,
  MessageCreatePayload,
  SessionMessageResponse,
  SessionPagingOptions,
  SessionResponse,
  SessionUpdatePayload,
  SessionUpsertPayload,
  TurnRuntimeSnapshotLookupResponse,
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
  deleteSession(id: string): Promise<DeleteSuccessResponse>;
  getConversationMessages(
    conversationId: string,
    params?: { limit?: number; offset?: number; compact?: boolean; strategy?: string },
  ): Promise<SessionMessageResponse[]>;
  getConversationTurnMessages(conversationId: string, userMessageId: string): Promise<SessionMessageResponse[]>;
  getConversationTurnProcessMessages(conversationId: string, userMessageId: string): Promise<SessionMessageResponse[]>;
  getConversationTurnProcessMessagesByTurn(conversationId: string, turnId: string): Promise<SessionMessageResponse[]>;
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
  async deleteSession(id) {
    return workspaceApi.deleteSession(this.getRequestFn(), id);
  },
  async getConversationMessages(conversationId, params) {
    return workspaceApi.getConversationMessages(this.getRequestFn(), conversationId, params);
  },
  async getConversationTurnMessages(conversationId, userMessageId) {
    return workspaceApi.getConversationTurnMessages(this.getRequestFn(), conversationId, userMessageId);
  },
  async getConversationTurnProcessMessages(conversationId, userMessageId) {
    return workspaceApi.getConversationTurnProcessMessages(this.getRequestFn(), conversationId, userMessageId);
  },
  async getConversationTurnProcessMessagesByTurn(conversationId, turnId) {
    return workspaceApi.getConversationTurnProcessMessagesByTurn(this.getRequestFn(), conversationId, turnId);
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
