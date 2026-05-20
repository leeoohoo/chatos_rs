import * as configsApi from '../configs';
import * as conversationApi from '../conversation';
import * as memoryApi from '../memory';
import type {
  AiCreateAgentResponse,
  AiCreateAgentPayload,
  AiModelConfigCreatePayload,
  AiModelConfigResponse,
  AiModelConfigUpdatePayload,
  ActiveSystemContextResponse,
  ApplicationCreatePayload,
  ApplicationResponse,
  ApplicationUpdatePayload,
  CreateAgentPayload,
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
  MemoryAgentSessionResponse,
  MemoryAgentRuntimeContextResponse,
  MemoryAgentSessionsQueryOptions,
  MemorySkillsQueryOptions,
  MemorySkillPluginResponse,
  MemorySkillResponse,
  SystemContextCreatePayload,
  SystemContextDraftEvaluatePayload,
  SystemContextDraftEvaluateResponse,
  SystemContextDraftGeneratePayload,
  SystemContextDraftGenerateResponse,
  SystemContextDraftOptimizePayload,
  SystemContextDraftOptimizeResponse,
  SystemContextResponse,
  SystemContextUpdatePayload,
  UpdateAgentPayload,
} from '../types';
import type ApiClient from '../../client';

export interface ConfigFacade {
  getMcpConfigs(userId?: string, options?: { forceRefresh?: boolean }): Promise<McpConfigResponse[]>;
  createMcpConfig(data: McpConfigCreatePayload): Promise<McpConfigResponse>;
  updateMcpConfig(id: string, data: McpConfigUpdatePayload): Promise<McpConfigResponse>;
  deleteMcpConfig(id: string): Promise<{ success?: boolean }>;
  getAiModelConfigs(): Promise<AiModelConfigResponse[]>;
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
  getAgents(userId?: string, options?: MemoryAgentsQueryOptions): Promise<MemoryAgentResponse[]>;
  getAgentSessions(
    agentId: string,
    userId?: string,
    options?: MemoryAgentSessionsQueryOptions,
  ): Promise<MemoryAgentSessionResponse[]>;
  getAgentRuntimeContext(agentId: string): Promise<MemoryAgentRuntimeContextResponse>;
  createAgent(data: CreateAgentPayload): Promise<MemoryAgentResponse>;
  updateAgent(agentId: string, data: UpdateAgentPayload): Promise<MemoryAgentResponse>;
  deleteAgent(agentId: string): Promise<{ success?: boolean }>;
  aiCreateAgent(data: AiCreateAgentPayload): Promise<AiCreateAgentResponse>;
  listSkillPlugins(userId?: string, options?: { limit?: number; offset?: number }): Promise<MemorySkillPluginResponse[]>;
  listSkills(userId?: string, options?: MemorySkillsQueryOptions): Promise<MemorySkillResponse[]>;
  getSkill(skillId: string): Promise<MemorySkillResponse>;
  getSkillPlugin(source: string): Promise<MemorySkillPluginResponse>;
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
  async getMcpConfigs(userId, options) {
    return configsApi.getMcpConfigs(this.getRequestFn(), userId, options);
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
  async getAiModelConfigs() {
    return configsApi.getAiModelConfigs(this.getRequestFn());
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
  async getAgents(userId, options) {
    return memoryApi.getAgents(this.getRequestFn(), userId, options);
  },
  async getAgentSessions(agentId, userId, options) {
    return memoryApi.getAgentSessions(this.getRequestFn(), agentId, userId, options);
  },
  async getAgentRuntimeContext(agentId) {
    return memoryApi.getAgentRuntimeContext(this.getRequestFn(), agentId);
  },
  async createAgent(data) {
    return memoryApi.createAgent(this.getRequestFn(), data);
  },
  async updateAgent(agentId, data) {
    return memoryApi.updateAgent(this.getRequestFn(), agentId, data);
  },
  async deleteAgent(agentId) {
    return memoryApi.deleteAgent(this.getRequestFn(), agentId);
  },
  async aiCreateAgent(data) {
    return memoryApi.aiCreateAgent(this.getRequestFn(), data);
  },
  async listSkillPlugins(userId, options) {
    return memoryApi.listSkillPlugins(this.getRequestFn(), userId, options);
  },
  async listSkills(userId, options) {
    return memoryApi.listSkills(this.getRequestFn(), userId, options);
  },
  async getSkill(skillId) {
    return memoryApi.getSkill(this.getRequestFn(), skillId);
  },
  async getSkillPlugin(source) {
    return memoryApi.getSkillPlugin(this.getRequestFn(), source);
  },
  async getMemoryAgents(userId, options) {
    return this.getAgents(userId, options);
  },
  async getMemoryAgentRuntimeContext(agentId) {
    return this.getAgentRuntimeContext(agentId);
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
