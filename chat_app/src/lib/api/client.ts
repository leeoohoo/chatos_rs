// API客户端，用于连接后端服务
import {
  ipcAvailable,
  streamChatIPC,
  streamAgentChatIPC,
  stopChatIPC,
  listSessionsIPC,
  createSessionIPC,
  getSessionIPC,
  deleteSessionIPC,
  getSessionMessagesIPC,
  createMessageIPC,
  getUserSettingsIPC,
  updateUserSettingsIPC
} from '../ipc/transport';
import { debugLog } from '@/lib/utils';
// 使用相对路径，让浏览器自动处理协议和域名
const API_BASE_URL = '/api';

class ApiClient {
  private baseUrl: string;

  constructor(baseUrl: string = API_BASE_URL) {
    this.baseUrl = baseUrl;
  }

  getBaseUrl(): string {
    return this.baseUrl;
  }

  private async request<T>(endpoint: string, options: RequestInit = {}): Promise<T> {
    const url = `${this.baseUrl}${endpoint}`;
    const config: RequestInit = {
      headers: {
        'Content-Type': 'application/json',
        ...options.headers,
      },
      ...options,
    };

    try {
      const response = await fetch(url, config);
      
      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`);
      }
      
      // 检查响应是否有内容
      const text = await response.text();
      if (!text) {
        return {} as T; // 返回空对象而不是尝试解析空字符串
      }
      
      try {
        return JSON.parse(text);
      } catch (parseError) {
        console.error(`JSON parse error for ${endpoint}:`, parseError, 'Response text:', text);
        throw new Error(`Invalid JSON response: ${text}`);
      }
    } catch (error) {
      console.error(`API request failed: ${endpoint}`, error);
      throw error;
    }
  }

  // 会话相关API
  async getSessions(
    userId?: string,
    projectId?: string,
    paging?: { limit?: number; offset?: number }
  ): Promise<any[]> {
    if (ipcAvailable()) {
      const all = await listSessionsIPC(userId, projectId);
      if (paging?.limit !== undefined) {
        const offset = paging?.offset || 0;
        const end = offset + paging.limit;
        return all.slice(offset, end);
      }
      return all;
    }
    const params = new URLSearchParams();
    if (userId) params.append('user_id', userId);  // 修复：使用user_id匹配后端参数名
    if (projectId) params.append('project_id', projectId);  // 修复：使用project_id匹配后端参数名
    if (paging?.limit !== undefined) params.append('limit', String(paging.limit));
    if (paging?.offset !== undefined) params.append('offset', String(paging.offset));
    const queryString = params.toString();
    debugLog('🔍 getSessions API调用:', { userId, projectId, queryString });
    return this.request<any[]>(`/sessions${queryString ? `?${queryString}` : ''}`);
  }

  async createSession(data: { id: string; title: string; user_id: string; project_id: string }): Promise<any> {
    debugLog('🔍 createSession API调用:', data);
    if (ipcAvailable()) {
      return createSessionIPC(data);
    }
    return this.request<any>('/sessions', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async getSession(id: string): Promise<any> {
    if (ipcAvailable()) return getSessionIPC(id);
    return this.request<any>(`/sessions/${id}`);
  }

  async deleteSession(id: string): Promise<any> {
    if (ipcAvailable()) {
      return deleteSessionIPC(id);
    }
    return this.request<any>(`/sessions/${id}`, {
      method: 'DELETE',
    });
  }

  async getSessionMessages(sessionId: string, params?: { limit?: number; offset?: number }): Promise<any[]> {
    if (ipcAvailable()) {
      return getSessionMessagesIPC(sessionId, params);
    }
    const qs: string[] = [];
    if (params?.limit !== undefined) qs.push(`limit=${encodeURIComponent(String(params.limit))}`);
    if (params?.offset !== undefined) qs.push(`offset=${encodeURIComponent(String(params.offset))}`);
    const query = qs.length ? `?${qs.join('&')}` : '';
    return this.request<any[]>(`/sessions/${sessionId}/messages${query}`);
  }

  // 消息相关API
  async createMessage(data: {
    id: string;
    sessionId: string;
    role: string;
    content: string;
    metadata?: any;
    toolCalls?: any[];
    createdAt?: Date;
    status?: string;
  }): Promise<any> {
    const requestData = {
      ...data,
      createdAt: data.createdAt ? data.createdAt.toISOString() : undefined
    };
    if (ipcAvailable()) {
      const payload = {
        sessionId: data.sessionId,
        role: data.role,
        content: data.content,
        toolCalls: data.toolCalls,
        toolCallId: (data as any).toolCallId,
        reasoning: (data as any).reasoning,
        metadata: data.metadata
      };
      return createMessageIPC(payload);
    }
    return this.request<any>(`/sessions/${data.sessionId}/messages`, {
      method: 'POST',
      body: JSON.stringify(requestData),
    });
  }

  // MCP配置相关API
  async getMcpConfigs(userId?: string) {
    if (ipcAvailable()) {
      const { getMcpConfigsIPC } = await import('../ipc/transport');
      return getMcpConfigsIPC(userId);
    }
    const params = userId ? `?user_id=${encodeURIComponent(userId)}` : '';
    debugLog('🔍 getMcpConfigs API调用:', { userId, params });
    return this.request(`/mcp-configs${params}`);
  }

  async createMcpConfig(data: {
    id: string;
    name: string;
    command: string;
    type: 'http' | 'stdio';
    args?: string[] | null;
    env?: Record<string, string> | null;
    cwd?: string | null;
    enabled: boolean;
    user_id?: string;
  }) {
    debugLog('🔍 API client createMcpConfig 调用:', data);
    if (ipcAvailable()) {
      const { createMcpConfigIPC } = await import('../ipc/transport');
      return createMcpConfigIPC(data as any);
    }
    return this.request('/mcp-configs', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async updateMcpConfig(id: string, data: {
    id?: string;
    name?: string;
    command?: string;
    type?: 'http' | 'stdio';
    args?: string[] | null;
    env?: Record<string, string> | null;
    cwd?: string | null;
    enabled?: boolean;
    userId?: string;
  }) {
    debugLog('🔍 API client updateMcpConfig 调用:', { id, data });
    if (ipcAvailable()) {
      const { updateMcpConfigIPC } = await import('../ipc/transport');
      return updateMcpConfigIPC(id, data);
    }
    return this.request(`/mcp-configs/${id}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  }

  async deleteMcpConfig(id: string) {
    if (ipcAvailable()) {
      const { deleteMcpConfigIPC } = await import('../ipc/transport');
      return deleteMcpConfigIPC(id);
    }
    return this.request(`/mcp-configs/${id}`, {
      method: 'DELETE',
    });
  }

  // AI模型配置相关API
  async getAiModelConfigs(userId?: string) {
    if (ipcAvailable()) {
      const { getAiModelConfigsIPC } = await import('../ipc/transport');
      return getAiModelConfigsIPC(userId);
    }
    const params = userId ? `?user_id=${encodeURIComponent(userId)}` : '';
    debugLog('🔍 getAiModelConfigs API调用:', { userId, params });
    return this.request(`/ai-model-configs${params}`);
  }

  async createAiModelConfig(data: {
    id: string;
    name: string;
    provider: string;
    model: string;
    thinking_level?: string;
    api_key: string;
    base_url: string;
    user_id?: string;
    enabled: boolean;
    supports_images?: boolean;
    supports_reasoning?: boolean;
    supports_responses?: boolean;
  }) {
    if (ipcAvailable()) {
      const { createAiModelConfigIPC } = await import('../ipc/transport');
      return createAiModelConfigIPC(data);
    }
    return this.request('/ai-model-configs', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async updateAiModelConfig(id: string, data: any) {
    if (ipcAvailable()) {
      const { updateAiModelConfigIPC } = await import('../ipc/transport');
      return updateAiModelConfigIPC(id, data);
    }
    return this.request(`/ai-model-configs/${id}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  }

  async deleteAiModelConfig(id: string) {
    if (ipcAvailable()) {
      const { deleteAiModelConfigIPC } = await import('../ipc/transport');
      return deleteAiModelConfigIPC(id);
    }
    return this.request(`/ai-model-configs/${id}`, {
      method: 'DELETE',
    });
  }

  // 系统上下文相关API
  async getSystemContexts(userId: string): Promise<any[]> {
    if (ipcAvailable()) {
      const { getSystemContextsIPC } = await import('../ipc/transport');
      return getSystemContextsIPC(userId);
    }
    return this.request<any[]>(`/system-contexts?user_id=${userId}`);
  }

  async getActiveSystemContext(userId: string): Promise<{ content: string; context: any }> {
    if (ipcAvailable()) {
      const { getActiveSystemContextIPC } = await import('../ipc/transport');
      return getActiveSystemContextIPC(userId);
    }
    return this.request<{ content: string; context: any }>(`/system-context/active?user_id=${userId}`);
  }

  async createSystemContext(data: {
    name: string;
    content: string;
    user_id: string;
    app_ids?: string[];
  }): Promise<any> {
    debugLog('🔍 API client createSystemContext 调用:', data);
    debugLog('🔍 [关键] app_ids 字段:', data.app_ids, '类型:', typeof data.app_ids, '是否为数组:', Array.isArray(data.app_ids));
    if (ipcAvailable()) {
      const { createSystemContextIPC } = await import('../ipc/transport');
      return createSystemContextIPC(data);
    }
    return this.request<any>('/system-contexts', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async updateSystemContext(id: string, data: {
    name: string;
    content: string;
    app_ids?: string[];
  }): Promise<any> {
    debugLog('🔍 API client updateSystemContext 调用:', { id, data });
    debugLog('🔍 [关键] app_ids 字段:', data.app_ids, '类型:', typeof data.app_ids, '是否为数组:', Array.isArray(data.app_ids));
    if (ipcAvailable()) {
      const { updateSystemContextIPC } = await import('../ipc/transport');
      return updateSystemContextIPC(id, data);
    }
    return this.request<any>(`/system-contexts/${id}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  }

  async deleteSystemContext(id: string): Promise<void> {
    if (ipcAvailable()) {
      const { deleteSystemContextIPC } = await import('../ipc/transport');
      await deleteSystemContextIPC(id);
      return;
    }
    return this.request<void>(`/system-contexts/${id}`, {
      method: 'DELETE',
    });
  }

  async activateSystemContext(id: string, userId: string): Promise<any> {
    if (ipcAvailable()) {
      const { activateSystemContextIPC } = await import('../ipc/transport');
      return activateSystemContextIPC(id, userId);
    }
    return this.request<any>(`/system-contexts/${id}/activate`, {
      method: 'POST',
      body: JSON.stringify({ user_id: userId, is_active: true }),
    });
  }

  // 应用（Application）相关API
  async getApplications(userId?: string): Promise<any[]> {
    if (ipcAvailable()) {
      const { getApplicationsIPC } = await import('../ipc/transport');
      return getApplicationsIPC(userId);
    }
    const params = userId ? `?user_id=${encodeURIComponent(userId)}` : '';
    return this.request<any[]>(`/applications${params}`);
  }

  async getApplication(id: string): Promise<any> {
    if (ipcAvailable()) {
      const { getApplicationIPC } = await import('../ipc/transport');
      return getApplicationIPC(id);
    }
    return this.request<any>(`/applications/${id}`);
  }

  async createApplication(data: {
    name: string;
    url: string;
    icon_url?: string | null;
    user_id?: string;
  }): Promise<any> {
    if (ipcAvailable()) {
      const { createApplicationIPC } = await import('../ipc/transport');
      return createApplicationIPC(data as any);
    }
    return this.request<any>('/applications', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async updateApplication(id: string, data: {
    name?: string;
    url?: string;
    icon_url?: string | null;
  }): Promise<any> {
    if (ipcAvailable()) {
      const { updateApplicationIPC } = await import('../ipc/transport');
      return updateApplicationIPC(id, data as any);
    }
    return this.request<any>(`/applications/${id}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  }

  async deleteApplication(id: string): Promise<any> {
    if (ipcAvailable()) {
      const { deleteApplicationIPC } = await import('../ipc/transport');
      return deleteApplicationIPC(id);
    }
    return this.request<any>(`/applications/${id}`, {
      method: 'DELETE',
    });
  }

  // 智能体（Agent）相关API
  async getAgents(userId?: string): Promise<any[]> {
    if (ipcAvailable()) {
      const { getAgentsIPC } = await import('../ipc/transport');
      return getAgentsIPC(userId);
    }
    const params = userId ? `?user_id=${encodeURIComponent(userId)}` : '';
    return this.request<any[]>(`/agents${params}`);
  }

  async createAgent(data: {
    name: string;
    description?: string;
    ai_model_config_id: string;
    mcp_config_ids?: string[];
    callable_agent_ids?: string[];
    system_context_id?: string;
    workspace_dir?: string | null;
    user_id?: string;
    enabled?: boolean;
    app_ids?: string[];
  }): Promise<any> {
    debugLog('🔍 API client createAgent 调用:', data);
    debugLog('🔍 [关键] app_ids 字段:', data.app_ids, '类型:', typeof data.app_ids, '是否为数组:', Array.isArray(data.app_ids));
    if (ipcAvailable()) {
      const { createAgentIPC } = await import('../ipc/transport');
      return createAgentIPC(data as any);
    }
    return this.request<any>('/agents', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async updateAgent(agentId: string, data: {
    name?: string;
    description?: string;
    ai_model_config_id?: string;
    mcp_config_ids?: string[];
    callable_agent_ids?: string[];
    system_context_id?: string;
    workspace_dir?: string | null;
    enabled?: boolean;
    app_ids?: string[];
  }): Promise<any> {
    debugLog('🔍 API client updateAgent 调用:', { agentId, data });
    debugLog('🔍 [关键] app_ids 字段:', data.app_ids, '类型:', typeof data.app_ids, '是否为数组:', Array.isArray(data.app_ids));
    if (ipcAvailable()) {
      const { updateAgentIPC } = await import('../ipc/transport');
      return updateAgentIPC(agentId, data as any);
    }
    return this.request<any>(`/agents/${agentId}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  }

  async deleteAgent(agentId: string): Promise<any> {
    if (ipcAvailable()) {
      const { deleteAgentIPC } = await import('../ipc/transport');
      return deleteAgentIPC(agentId);
    }
    return this.request<any>(`/agents/${agentId}`, {
      method: 'DELETE',
    });
  }

  // 会话详情和助手相关API (从index.ts合并)
  async getConversationDetails(conversationId: string) {
    try {
      const session = await this.request<any>(`/sessions/${conversationId}`);
      return {
        data: {
          conversation: {
            id: session.id,
            title: session.title,
            created_at: session.created_at,
            updated_at: session.updated_at
          }
        }
      };
    } catch (error) {
      console.error('Failed to get conversation details:', error);
      // 返回默认值以保持兼容性
      return {
        data: {
          conversation: {
            id: conversationId,
            title: 'Default Conversation',
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString()
          }
        }
      };
    }
  }

  async getAssistant(_conversationId: string) {
    try {
      // 获取AI模型配置
      const configs = await this.request<any[]>('/ai-model-configs');
      const defaultConfig = configs.find((config: any) => config.enabled) || configs[0];
      
      if (!defaultConfig) {
        throw new Error('No AI model configuration found');
      }

      return {
        data: {
          assistant: {
            id: defaultConfig.id,
            name: defaultConfig.name,
            model_config: {
              model_name: defaultConfig.model_name,
              temperature: 0.7,
              api_key: defaultConfig.api_key,
              base_url: defaultConfig.base_url
            }
          }
        }
      };
    } catch (error) {
      console.error('Failed to get assistant:', error);
      // 返回默认值以保持兼容性
      return {
        data: {
          assistant: {
            id: 'default-assistant',
            name: 'AI Assistant',
            model_config: {
              model_name: 'gpt-3.5-turbo',
              temperature: 0.7,
              // 避免对 import.meta.env 的类型依赖以通过声明生成
              api_key: '',
              base_url: 'https://api.openai.com/v1'
            }
          }
        }
      };
    }
  }

  async getMcpServers(_conversationId?: string) {
    try {
      // 直接获取全局MCP配置，而不是基于会话的配置
      const mcpConfigs = await this.request<any[]>('/mcp-configs');
      // 只返回启用的MCP服务器，并转换数据格式
      const enabledServers = mcpConfigs
        .filter((config: any) => config.enabled)
        .map((config: any) => ({
          name: config.name,
          url: config.command // 后端使用command字段存储URL
        }));
      return {
        data: {
          mcp_servers: enabledServers
        }
      };
    } catch (error) {
      console.error('Failed to get MCP servers:', error);
      return {
        data: {
          mcp_servers: []
        }
      };
    }
  }

  async getMcpConfigResource(configId: string): Promise<{ success: boolean; config: any; alias?: string }> {
    try {
      if (ipcAvailable()) {
        const { getMcpConfigResourceIPC } = await import('../ipc/transport');
        return await getMcpConfigResourceIPC(configId);
      }
      const res = await this.request<any>(`/mcp-configs/${configId}/resource/config`);
      return res;
    } catch (error) {
      console.error('Failed to get MCP config resource:', error);
      return { success: false, config: null } as any;
    }
  }

  async getMcpConfigResourceByCommand(data: {
    type: 'stdio' | 'http';
    command: string;
    args?: string[] | null;
    env?: Record<string, string> | null;
    cwd?: string | null;
    alias?: string | null;
  }): Promise<{ success: boolean; config: any; alias?: string }> {
    try {
      if (ipcAvailable()) {
        const { getMcpConfigResourceByCommandIPC } = await import('../ipc/transport');
        return await getMcpConfigResourceByCommandIPC(data);
      }
      const res = await this.request<any>(`/mcp-configs/resource/config`, {
        method: 'POST',
        body: JSON.stringify(data),
      });
      return res;
    } catch (error) {
      console.error('Failed to get MCP config resource by command:', error);
      return { success: false, config: null } as any;
    }
  }
  async saveMessage(conversationId: string, message: any) {
    try {
      // 生成唯一ID
      const messageId = message.id || `msg_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
      
      const savedMessage = await this.request<any>(`/messages`, {
        method: 'POST',
        body: JSON.stringify({
          id: messageId,
          sessionId: conversationId,
          role: message.role,
          content: message.content,
          toolCalls: message.tool_calls || null,
          toolCallId: message.tool_call_id || null,
          reasoning: message.reasoning || null,
          metadata: message.metadata || null
        })
      });
      
      return {
        data: {
          message: savedMessage
        }
      };
    } catch (error) {
      console.error('Failed to save message:', error);
      // 返回模拟数据以保持兼容性
      return {
        data: {
          message: {
            ...message,
            id: Date.now().toString(),
            created_at: new Date().toISOString()
          }
        }
      };
    }
  }

  async getMessages(conversationId: string, params: { limit?: number; offset?: number } = {}) {
    try {
      const qs: string[] = [];
      if (params.limit !== undefined) qs.push(`limit=${encodeURIComponent(String(params.limit))}`);
      if (params.offset !== undefined) qs.push(`offset=${encodeURIComponent(String(params.offset))}`);
      const query = qs.length ? `?${qs.join('&')}` : '';
      const messages = await this.request<any[]>(`/sessions/${conversationId}/messages${query}`);
      return {
        data: {
          messages: messages
        }
      };
    } catch (error) {
      console.error('Failed to get messages:', error);
      return {
        data: {
          messages: []
        }
      };
    }
  }

  async addMessage(conversationId: string, message: any) {
    return this.saveMessage(conversationId, message);
  }

  // 流式聊天接口
  async streamChat(sessionId: string, content: string, modelConfig: any, userId?: string, attachments?: any[], reasoningEnabled?: boolean): Promise<ReadableStream> {
    // Prefer IPC transport when available (Electron). Fallback to HTTP/SSE.
    if (ipcAvailable()) {
      return streamChatIPC(sessionId, content, modelConfig, userId, attachments, reasoningEnabled);
    }
    const useResponses = modelConfig?.supports_responses === true;
    const url = `${this.baseUrl}/${useResponses ? 'agent_v3' : 'agent_v2'}/chat/stream`;
    
    const response = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        session_id: sessionId,
        content: content,
        user_id: userId,
        attachments: attachments || [],
        reasoning_enabled: reasoningEnabled,
        ai_model_config: {
          provider: modelConfig.provider,
          model_name: modelConfig.model_name,
          temperature: modelConfig.temperature || 0.7,
          thinking_level: modelConfig.thinking_level,
          api_key: modelConfig.api_key,
          base_url: modelConfig.base_url,
          supports_images: modelConfig.supports_images === true,
          supports_reasoning: modelConfig.supports_reasoning === true,
          supports_responses: modelConfig.supports_responses === true
        }
      }),
    });

    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }

    if (!response.body) {
      throw new Error('Response body is null');
    }

    return response.body;
  }

  async streamAgentChat(
    sessionId: string,
    content: string,
    agentId: string,
    userId?: string,
    attachments?: any[],
    reasoningEnabled?: boolean,
    options?: { useResponses?: boolean }
  ): Promise<ReadableStream> {
    if (ipcAvailable()) {
      return streamAgentChatIPC(sessionId, content, agentId, userId, attachments, reasoningEnabled, options);
    }
    const useResponses = options?.useResponses === true;
    const url = `${this.baseUrl}/${useResponses ? 'agent_v3/agents' : 'agents'}/chat/stream`;

    const response = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Accept': 'text/event-stream',
      },
      body: JSON.stringify({
        session_id: sessionId,
        content: content,
        agent_id: agentId,
        user_id: userId,
        attachments: attachments || [],
        reasoning_enabled: reasoningEnabled,
      }),
    });

    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }

    if (!response.body) {
      throw new Error('Response body is null');
    }

    return response.body;
  }

  // 停止聊天流
  async stopChat(sessionId: string, options?: { useResponses?: boolean }): Promise<any> {
    if (ipcAvailable()) {
      await stopChatIPC(sessionId);
      return { ok: true } as any;
    }
    const useResponses = options?.useResponses === true;
    const path = useResponses ? '/agent_v3/chat/stop' : '/chat/stop';
    return this.request<any>(path, {
      method: 'POST',
      body: JSON.stringify({
        session_id: sessionId
      }),
    });
  }

  // User settings APIs
  async getUserSettings(userId?: string): Promise<any> {
    if (ipcAvailable()) {
      return getUserSettingsIPC(userId);
    }
    const qs = userId ? `?user_id=${encodeURIComponent(userId)}` : '';
    return this.request<any>(`/user-settings${qs}`);
  }

  async updateUserSettings(userId: string, settings: Record<string, any>): Promise<any> {
    if (ipcAvailable()) {
      return updateUserSettingsIPC(userId, settings);
    }
    return this.request<any>(`/user-settings`, {
      method: 'PUT',
      body: JSON.stringify({ user_id: userId, settings })
    });
  }
}

// 导出单例实例
export const apiClient = new ApiClient();

// 为了保持向后兼容性，导出conversationsApi对象
export const conversationsApi = {
  getDetails: (conversationId: string) => apiClient.getConversationDetails(conversationId),
  getAssistant: (conversationId: string) => apiClient.getAssistant(conversationId),
  getMcpServers: (conversationId?: string) => apiClient.getMcpServers(conversationId),
  saveMessage: (conversationId: string, message: any) => apiClient.saveMessage(conversationId, message),
  getMessages: (conversationId: string, params?: any) => apiClient.getMessages(conversationId, params),
  addMessage: (conversationId: string, message: any) => apiClient.addMessage(conversationId, message)
};

export default ApiClient;
