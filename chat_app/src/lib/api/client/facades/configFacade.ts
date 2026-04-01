import * as configsApi from '../configs';
import * as conversationApi from '../conversation';
import * as memoryApi from '../memory';
import type {
  AiModelConfigCreatePayload,
  AiModelConfigResponse,
  AiModelConfigUpdatePayload,
  ActiveSystemContextResponse,
  ApplicationCreatePayload,
  ApplicationResponse,
  ApplicationUpdatePayload,
  ConversationAssistantResponse,
  ConversationDetailsResponse,
  ConversationMessageEnvelope,
  ConversationMessagePayload,
  ConversationMessagesEnvelope,
  ConversationMcpServersResponse,
  McpConfigCreatePayload,
  McpConfigResourceResponse,
  McpConfigResponse,
  McpConfigUpdatePayload,
  MemoryAgentsQueryOptions,
  MemoryAgentResponse,
  MemoryAgentRuntimeContextResponse,
  SystemContextCreatePayload,
  SystemContextDraftEvaluatePayload,
  SystemContextDraftEvaluateResponse,
  SystemContextDraftGeneratePayload,
  SystemContextDraftGenerateResponse,
  SystemContextDraftOptimizePayload,
  SystemContextDraftOptimizeResponse,
  SystemContextResponse,
  SystemContextUpdatePayload,
} from '../types';
import type ApiClient from '../../client';

export interface ConfigFacade {
  getMcpConfigs(userId?: string): Promise<McpConfigResponse[]>;
  createMcpConfig(data: McpConfigCreatePayload): Promise<McpConfigResponse>;
  updateMcpConfig(id: string, data: McpConfigUpdatePayload): Promise<McpConfigResponse>;
  deleteMcpConfig(id: string): Promise<{ success?: boolean }>;
  getAiModelConfigs(userId?: string): Promise<AiModelConfigResponse[]>;
  createAiModelConfig(data: AiModelConfigCreatePayload): Promise<AiModelConfigResponse>;
  updateAiModelConfig(id: string, data: AiModelConfigUpdatePayload): Promise<AiModelConfigResponse>;
  deleteAiModelConfig(id: string): Promise<{ success?: boolean }>;
  getSystemContexts(userId: string): Promise<SystemContextResponse[]>;
  getActiveSystemContext(userId: string): Promise<ActiveSystemContextResponse>;
  createSystemContext(data: SystemContextCreatePayload): Promise<SystemContextResponse>;
  updateSystemContext(id: string, data: SystemContextUpdatePayload): Promise<SystemContextResponse>;
  deleteSystemContext(id: string): Promise<void>;
  activateSystemContext(id: string, userId: string): Promise<SystemContextResponse>;
  generateSystemContextDraft(data: SystemContextDraftGeneratePayload): Promise<SystemContextDraftGenerateResponse>;
  optimizeSystemContextDraft(data: SystemContextDraftOptimizePayload): Promise<SystemContextDraftOptimizeResponse>;
  evaluateSystemContextDraft(data: SystemContextDraftEvaluatePayload): Promise<SystemContextDraftEvaluateResponse>;
  getApplications(userId?: string): Promise<ApplicationResponse[]>;
  getApplication(id: string): Promise<ApplicationResponse>;
  createApplication(data: ApplicationCreatePayload): Promise<ApplicationResponse>;
  updateApplication(id: string, data: ApplicationUpdatePayload): Promise<ApplicationResponse>;
  deleteApplication(id: string): Promise<{ success?: boolean }>;
  getMemoryAgents(userId?: string, options?: MemoryAgentsQueryOptions): Promise<MemoryAgentResponse[]>;
  getMemoryAgentRuntimeContext(agentId: string): Promise<MemoryAgentRuntimeContextResponse>;
  getConversationDetails(conversationId: string): Promise<ConversationDetailsResponse>;
  getAssistant(conversationId: string): Promise<ConversationAssistantResponse>;
  getMcpServers(conversationId?: string): Promise<ConversationMcpServersResponse>;
  getMcpConfigResource(configId: string): Promise<McpConfigResourceResponse>;
  getMcpConfigResourceByCommand(data: {
    type: 'stdio' | 'http';
    command: string;
    args?: string[] | null;
    env?: Record<string, string> | null;
    cwd?: string | null;
    alias?: string | null;
  }): Promise<McpConfigResourceResponse>;
  saveMessage(conversationId: string, message: ConversationMessagePayload): Promise<ConversationMessageEnvelope>;
  getMessages(
    conversationId: string,
    params?: { limit?: number; offset?: number },
  ): Promise<ConversationMessagesEnvelope>;
  addMessage(conversationId: string, message: ConversationMessagePayload): Promise<ConversationMessageEnvelope>;
}

export const configFacade: ConfigFacade & ThisType<ApiClient> = {
  async getMcpConfigs(userId) {
    return configsApi.getMcpConfigs(this.getRequestFn(), userId);
  },
  async createMcpConfig(data) {
    return configsApi.createMcpConfig(this.getRequestFn(), data);
  },
  async updateMcpConfig(id, data) {
    return configsApi.updateMcpConfig(this.getRequestFn(), id, data);
  },
  async deleteMcpConfig(id) {
    return configsApi.deleteMcpConfig(this.getRequestFn(), id);
  },
  async getAiModelConfigs(userId) {
    return configsApi.getAiModelConfigs(this.getRequestFn(), userId);
  },
  async createAiModelConfig(data) {
    return configsApi.createAiModelConfig(this.getRequestFn(), data);
  },
  async updateAiModelConfig(id, data) {
    return configsApi.updateAiModelConfig(this.getRequestFn(), id, data);
  },
  async deleteAiModelConfig(id) {
    return configsApi.deleteAiModelConfig(this.getRequestFn(), id);
  },
  async getSystemContexts(userId) {
    return configsApi.getSystemContexts(this.getRequestFn(), userId);
  },
  async getActiveSystemContext(userId) {
    return configsApi.getActiveSystemContext(this.getRequestFn(), userId);
  },
  async createSystemContext(data) {
    return configsApi.createSystemContext(this.getRequestFn(), data);
  },
  async updateSystemContext(id, data) {
    return configsApi.updateSystemContext(this.getRequestFn(), id, data);
  },
  async deleteSystemContext(id) {
    return configsApi.deleteSystemContext(this.getRequestFn(), id);
  },
  async activateSystemContext(id, userId) {
    return configsApi.activateSystemContext(this.getRequestFn(), id, userId);
  },
  async generateSystemContextDraft(data) {
    return configsApi.generateSystemContextDraft(this.getRequestFn(), data);
  },
  async optimizeSystemContextDraft(data) {
    return configsApi.optimizeSystemContextDraft(this.getRequestFn(), data);
  },
  async evaluateSystemContextDraft(data) {
    return configsApi.evaluateSystemContextDraft(this.getRequestFn(), data);
  },
  async getApplications(userId) {
    return configsApi.getApplications(this.getRequestFn(), userId);
  },
  async getApplication(id) {
    return configsApi.getApplication(this.getRequestFn(), id);
  },
  async createApplication(data) {
    return configsApi.createApplication(this.getRequestFn(), data);
  },
  async updateApplication(id, data) {
    return configsApi.updateApplication(this.getRequestFn(), id, data);
  },
  async deleteApplication(id) {
    return configsApi.deleteApplication(this.getRequestFn(), id);
  },
  async getMemoryAgents(userId, options) {
    return memoryApi.getMemoryAgents(this.getRequestFn(), userId, options);
  },
  async getMemoryAgentRuntimeContext(agentId) {
    return memoryApi.getMemoryAgentRuntimeContext(this.getRequestFn(), agentId);
  },
  async getConversationDetails(conversationId) {
    return conversationApi.getConversationDetails(this.getRequestFn(), conversationId);
  },
  async getAssistant(conversationId) {
    return conversationApi.getAssistant(this.getRequestFn(), conversationId);
  },
  async getMcpServers(conversationId) {
    return conversationApi.getMcpServers(this.getRequestFn(), conversationId);
  },
  async getMcpConfigResource(configId) {
    return conversationApi.getMcpConfigResource(this.getRequestFn(), configId);
  },
  async getMcpConfigResourceByCommand(data) {
    return conversationApi.getMcpConfigResourceByCommand(this.getRequestFn(), data);
  },
  async saveMessage(conversationId, message) {
    return conversationApi.saveMessage(this.getRequestFn(), conversationId, message);
  },
  async getMessages(conversationId, params = {}) {
    return conversationApi.getMessages(this.getRequestFn(), conversationId, params);
  },
  async addMessage(conversationId, message) {
    return conversationApi.addMessage(this.getRequestFn(), conversationId, message);
  },
};
