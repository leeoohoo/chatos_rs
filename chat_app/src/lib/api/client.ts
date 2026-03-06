// API客户端，用于连接后端服务
import { buildQuery, guessFilenameFromPath, parseFilenameFromContentDisposition } from './client/shared';
import * as configsApi from './client/configs';
import * as workspaceApi from './client/workspace';
// 使用相对路径，让浏览器自动处理协议和域名
const API_BASE_URL = '/api';

class ApiClient {
  private baseUrl: string;
  private accessToken: string | null = null;
  private tokenRefreshListeners = new Set<(token: string) => void>();
  private readonly requestFn: workspaceApi.ApiRequestFn = (endpoint, options) => this.request(endpoint, options);

  constructor(baseUrl: string = API_BASE_URL) {
    this.baseUrl = baseUrl;
  }

  getBaseUrl(): string {
    return this.baseUrl;
  }

  setAccessToken(token?: string | null): void {
    const trimmed = (token || '').trim();
    this.accessToken = trimmed.length > 0 ? trimmed : null;
  }

  getAccessToken(): string | null {
    return this.accessToken;
  }

  onAccessTokenRefresh(listener: (token: string) => void): () => void {
    this.tokenRefreshListeners.add(listener);
    return () => this.tokenRefreshListeners.delete(listener);
  }

  private applyRefreshedAccessToken(response: Response): void {
    const refreshed = (response.headers.get('x-access-token') || '').trim();
    if (!refreshed || refreshed === this.accessToken) {
      return;
    }
    this.accessToken = refreshed;
    this.tokenRefreshListeners.forEach((listener) => {
      try {
        listener(refreshed);
      } catch (error) {
        console.error('Access token refresh listener failed:', error);
      }
    });
  }

  private async request<T>(endpoint: string, options: RequestInit = {}): Promise<T> {
    const url = `${this.baseUrl}${endpoint}`;
    const headers = new Headers(options.headers || {});
    if (!headers.has('Content-Type')) {
      headers.set('Content-Type', 'application/json');
    }
    if (this.accessToken && !headers.has('Authorization')) {
      headers.set('Authorization', `Bearer ${this.accessToken}`);
    }
    const config: RequestInit = {
      ...options,
      headers,
    };

    try {
      const response = await fetch(url, config);
      this.applyRefreshedAccessToken(response);
      const text = await response.text();
      let parsedBody: any = null;

      if (text) {
        try {
          parsedBody = JSON.parse(text);
        } catch (parseError) {
          if (response.ok) {
            console.error(`JSON parse error for ${endpoint}:`, parseError, 'Response text:', text);
            throw new Error(`Invalid JSON response: ${text}`);
          }
        }
      }

      if (!response.ok) {
        const errorMessage =
          (typeof parsedBody?.error === 'string' && parsedBody.error) ||
          (typeof parsedBody?.message === 'string' && parsedBody.message) ||
          `HTTP error! status: ${response.status}`;
        throw new Error(errorMessage);
      }

      if (!text) {
        return {} as T;
      }

      return parsedBody as T;
    } catch (error) {
      console.error(`API request failed: ${endpoint}`, error);
      throw error;
    }
  }

  // 会话相关API
  async getSessions(
    userId?: string,
    projectId?: string,
    paging?: { limit?: number; offset?: number; includeArchived?: boolean; includeArchiving?: boolean }
  ): Promise<any[]> {
    return workspaceApi.getSessions(this.requestFn, userId, projectId, paging);
  }

  async createSession(data: { id: string; title: string; user_id: string; project_id?: string; metadata?: any }): Promise<any> {
    return workspaceApi.createSession(this.requestFn, data);
  }

  async getSession(id: string): Promise<any> {
    return workspaceApi.getSession(this.requestFn, id);
  }

  async updateSession(
    id: string,
    data: { title?: string; description?: string; metadata?: any },
  ): Promise<any> {
    return workspaceApi.updateSession(this.requestFn, id, data);
  }

  async deleteSession(id: string): Promise<any> {
    return workspaceApi.deleteSession(this.requestFn, id);
  }

  async getSessionMessages(sessionId: string, params?: { limit?: number; offset?: number; compact?: boolean }): Promise<any[]> {
    return workspaceApi.getSessionMessages(this.requestFn, sessionId, params);
  }

  async getSessionTurnProcessMessages(sessionId: string, userMessageId: string): Promise<any[]> {
    return workspaceApi.getSessionTurnProcessMessages(this.requestFn, sessionId, userMessageId);
  }

  async getSessionTurnProcessMessagesByTurn(sessionId: string, turnId: string): Promise<any[]> {
    return workspaceApi.getSessionTurnProcessMessagesByTurn(this.requestFn, sessionId, turnId);
  }

  // 项目相关API
  async listProjects(userId?: string): Promise<any[]> {
    return workspaceApi.listProjects(this.requestFn, userId);
  }

  async createProject(data: { name: string; root_path: string; description?: string; user_id?: string }): Promise<any> {
    return workspaceApi.createProject(this.requestFn, data);
  }

  async updateProject(id: string, data: { name?: string; root_path?: string; description?: string }): Promise<any> {
    return workspaceApi.updateProject(this.requestFn, id, data);
  }

  async deleteProject(id: string): Promise<any> {
    return workspaceApi.deleteProject(this.requestFn, id);
  }

  async getProject(id: string): Promise<any> {
    return workspaceApi.getProject(this.requestFn, id);
  }

  async listProjectChangeLogs(
    projectId: string,
    params?: { path?: string; limit?: number; offset?: number }
  ): Promise<any[]> {
    return workspaceApi.listProjectChangeLogs(this.requestFn, projectId, params);
  }

  async getProjectChangeSummary(projectId: string): Promise<any> {
    return workspaceApi.getProjectChangeSummary(this.requestFn, projectId);
  }

  async confirmProjectChanges(
    projectId: string,
    payload: { mode?: 'all' | 'paths' | 'change_ids'; paths?: string[]; change_ids?: string[] }
  ): Promise<any> {
    return workspaceApi.confirmProjectChanges(this.requestFn, projectId, payload);
  }

  // 终端相关API
  async listTerminals(userId?: string): Promise<any[]> {
    return workspaceApi.listTerminals(this.requestFn, userId);
  }

  async createTerminal(data: { name?: string; cwd: string; user_id?: string }): Promise<any> {
    return workspaceApi.createTerminal(this.requestFn, data);
  }

  async getTerminal(id: string): Promise<any> {
    return workspaceApi.getTerminal(this.requestFn, id);
  }

  async deleteTerminal(id: string): Promise<any> {
    return workspaceApi.deleteTerminal(this.requestFn, id);
  }

  async listTerminalLogs(
    terminalId: string,
    params?: { limit?: number; offset?: number }
  ): Promise<any[]> {
    return workspaceApi.listTerminalLogs(this.requestFn, terminalId, params);
  }

  // 远端连接 API
  async listRemoteConnections(userId?: string): Promise<any[]> {
    return workspaceApi.listRemoteConnections(this.requestFn, userId);
  }

  async createRemoteConnection(data: {
    name?: string;
    host: string;
    port?: number;
    username: string;
    auth_type?: 'private_key' | 'private_key_cert' | 'password';
    password?: string;
    private_key_path?: string;
    certificate_path?: string;
    default_remote_path?: string;
    host_key_policy?: 'strict' | 'accept_new';
    jump_enabled?: boolean;
    jump_host?: string;
    jump_port?: number;
    jump_username?: string;
    jump_private_key_path?: string;
    jump_password?: string;
    user_id?: string;
  }): Promise<any> {
    return workspaceApi.createRemoteConnection(this.requestFn, data);
  }

  async getRemoteConnection(id: string): Promise<any> {
    return workspaceApi.getRemoteConnection(this.requestFn, id);
  }

  async updateRemoteConnection(id: string, data: {
    name?: string;
    host?: string;
    port?: number;
    username?: string;
    auth_type?: 'private_key' | 'private_key_cert' | 'password';
    password?: string;
    private_key_path?: string;
    certificate_path?: string;
    default_remote_path?: string;
    host_key_policy?: 'strict' | 'accept_new';
    jump_enabled?: boolean;
    jump_host?: string;
    jump_port?: number;
    jump_username?: string;
    jump_private_key_path?: string;
    jump_password?: string;
  }): Promise<any> {
    return workspaceApi.updateRemoteConnection(this.requestFn, id, data);
  }

  async deleteRemoteConnection(id: string): Promise<any> {
    return workspaceApi.deleteRemoteConnection(this.requestFn, id);
  }

  async disconnectRemoteTerminal(id: string): Promise<any> {
    return workspaceApi.disconnectRemoteTerminal(this.requestFn, id);
  }

  async testRemoteConnectionDraft(data: {
    name?: string;
    host: string;
    port?: number;
    username: string;
    auth_type?: 'private_key' | 'private_key_cert' | 'password';
    password?: string;
    private_key_path?: string;
    certificate_path?: string;
    default_remote_path?: string;
    host_key_policy?: 'strict' | 'accept_new';
    jump_enabled?: boolean;
    jump_host?: string;
    jump_port?: number;
    jump_username?: string;
    jump_private_key_path?: string;
    jump_password?: string;
    user_id?: string;
  }): Promise<any> {
    return workspaceApi.testRemoteConnectionDraft(this.requestFn, data);
  }

  async testRemoteConnection(id: string): Promise<any> {
    return workspaceApi.testRemoteConnection(this.requestFn, id);
  }

  async listRemoteSftpEntries(connectionId: string, path?: string): Promise<any> {
    return workspaceApi.listRemoteSftpEntries(this.requestFn, connectionId, path);
  }

  async uploadRemoteSftpFile(connectionId: string, localPath: string, remotePath: string): Promise<any> {
    return workspaceApi.uploadRemoteSftpFile(this.requestFn, connectionId, localPath, remotePath);
  }

  async downloadRemoteSftpFile(connectionId: string, remotePath: string, localPath: string): Promise<any> {
    return workspaceApi.downloadRemoteSftpFile(this.requestFn, connectionId, remotePath, localPath);
  }

  async startRemoteSftpTransfer(
    connectionId: string,
    data: {
      direction: 'upload' | 'download';
      local_path: string;
      remote_path: string;
    },
  ): Promise<any> {
    return workspaceApi.startRemoteSftpTransfer(this.requestFn, connectionId, data);
  }

  async getRemoteSftpTransferStatus(connectionId: string, transferId: string): Promise<any> {
    return workspaceApi.getRemoteSftpTransferStatus(this.requestFn, connectionId, transferId);
  }

  async cancelRemoteSftpTransfer(connectionId: string, transferId: string): Promise<any> {
    return workspaceApi.cancelRemoteSftpTransfer(this.requestFn, connectionId, transferId);
  }

  async createRemoteSftpDirectory(connectionId: string, parentPath: string, name: string): Promise<any> {
    return workspaceApi.createRemoteSftpDirectory(this.requestFn, connectionId, parentPath, name);
  }

  async renameRemoteSftpEntry(connectionId: string, fromPath: string, toPath: string): Promise<any> {
    return workspaceApi.renameRemoteSftpEntry(this.requestFn, connectionId, fromPath, toPath);
  }

  async deleteRemoteSftpEntry(connectionId: string, path: string, recursive = false): Promise<any> {
    return workspaceApi.deleteRemoteSftpEntry(this.requestFn, connectionId, path, recursive);
  }

  // 文件系统
  async listFsDirectories(path?: string): Promise<any> {
    return workspaceApi.listFsDirectories(this.requestFn, path);
  }

  async listFsEntries(path?: string): Promise<any> {
    return workspaceApi.listFsEntries(this.requestFn, path);
  }

  async searchFsEntries(path: string, query: string, limit?: number): Promise<any> {
    return workspaceApi.searchFsEntries(this.requestFn, path, query, limit);
  }

  async readFsFile(path: string): Promise<any> {
    return workspaceApi.readFsFile(this.requestFn, path);
  }

  async createFsDirectory(parentPath: string, name: string): Promise<any> {
    return workspaceApi.createFsDirectory(this.requestFn, parentPath, name);
  }

  async createFsFile(parentPath: string, name: string, content = ''): Promise<any> {
    return workspaceApi.createFsFile(this.requestFn, parentPath, name, content);
  }

  async deleteFsEntry(path: string, recursive = false): Promise<any> {
    return workspaceApi.deleteFsEntry(this.requestFn, path, recursive);
  }

  async moveFsEntry(
    sourcePath: string,
    targetParentPath: string,
    options?: { targetName?: string; replaceExisting?: boolean }
  ): Promise<any> {
    return workspaceApi.moveFsEntry(this.requestFn, sourcePath, targetParentPath, options);
  }

  async downloadFsEntry(path: string): Promise<{ blob: Blob; filename: string; contentType: string }> {
    const qs = `?path=${encodeURIComponent(path)}`;
    const headers = new Headers();
    if (this.accessToken) {
      headers.set('Authorization', `Bearer ${this.accessToken}`);
    }
    const response = await fetch(`${this.baseUrl}/fs/download${qs}`, {
      method: 'GET',
      headers,
    });

    this.applyRefreshedAccessToken(response);

    if (!response.ok) {
      const text = await response.text();
      let message = `HTTP error! status: ${response.status}`;
      if (text) {
        try {
          const parsed = JSON.parse(text);
          message =
            (typeof parsed?.error === 'string' && parsed.error) ||
            (typeof parsed?.message === 'string' && parsed.message) ||
            message;
        } catch {
          message = text;
        }
      }
      throw new Error(message);
    }

    const blob = await response.blob();
    const contentType = response.headers.get('content-type') || blob.type || 'application/octet-stream';
    const nameFromHeader = parseFilenameFromContentDisposition(response.headers.get('content-disposition'));
    let filename = nameFromHeader || guessFilenameFromPath(path);
    if (contentType.includes('application/zip') && !filename.toLowerCase().endsWith('.zip')) {
      filename = `${filename}.zip`;
    }
    return {
      blob,
      filename,
      contentType,
    };
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
    return this.request<any>(`/sessions/${data.sessionId}/messages`, {
      method: 'POST',
      body: JSON.stringify(requestData),
    });
  }

  // MCP配置相关API
  async getMcpConfigs(userId?: string) {
    return configsApi.getMcpConfigs(this.requestFn, userId);
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
    return configsApi.createMcpConfig(this.requestFn, data);
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
    return configsApi.updateMcpConfig(this.requestFn, id, data);
  }

  async deleteMcpConfig(id: string) {
    return configsApi.deleteMcpConfig(this.requestFn, id);
  }

  async getBuiltinMcpSettings(id: string): Promise<any> {
    return configsApi.getBuiltinMcpSettings(this.requestFn, id);
  }

  async getBuiltinMcpPermissions(id: string): Promise<any> {
    return configsApi.getBuiltinMcpPermissions(this.requestFn, id);
  }

  async updateBuiltinMcpPermissions(
    id: string,
    payload: { enabled_mcp_ids: string[]; selected_system_context_id?: string }
  ): Promise<any> {
    return configsApi.updateBuiltinMcpPermissions(this.requestFn, id, payload);
  }

  async importBuiltinMcpAgents(id: string, content: string): Promise<any> {
    return configsApi.importBuiltinMcpAgents(this.requestFn, id, content);
  }

  async importBuiltinMcpSkills(id: string, content: string): Promise<any> {
    return configsApi.importBuiltinMcpSkills(this.requestFn, id, content);
  }

  async importBuiltinMcpFromGit(
    id: string,
    payload: { repository: string; branch?: string; agents_path?: string; skills_path?: string }
  ): Promise<any> {
    return configsApi.importBuiltinMcpFromGit(this.requestFn, id, payload);
  }

  async installBuiltinMcpPlugin(
    id: string,
    payload: { source?: string; install_all?: boolean }
  ): Promise<any> {
    return configsApi.installBuiltinMcpPlugin(this.requestFn, id, payload);
  }

  // AI模型配置相关API
  async getAiModelConfigs(userId?: string) {
    return configsApi.getAiModelConfigs(this.requestFn, userId);
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
    return configsApi.createAiModelConfig(this.requestFn, data);
  }

  async updateAiModelConfig(id: string, data: any) {
    return configsApi.updateAiModelConfig(this.requestFn, id, data);
  }

  async deleteAiModelConfig(id: string) {
    return configsApi.deleteAiModelConfig(this.requestFn, id);
  }

  // 系统上下文相关API
  async getSystemContexts(userId: string): Promise<any[]> {
    return configsApi.getSystemContexts(this.requestFn, userId);
  }

  async getActiveSystemContext(userId: string): Promise<{ content: string; context: any }> {
    return configsApi.getActiveSystemContext(this.requestFn, userId);
  }

  async createSystemContext(data: {
    name: string;
    content: string;
    user_id: string;
    app_ids?: string[];
  }): Promise<any> {
    return configsApi.createSystemContext(this.requestFn, data);
  }

  async updateSystemContext(id: string, data: {
    name: string;
    content: string;
    app_ids?: string[];
  }): Promise<any> {
    return configsApi.updateSystemContext(this.requestFn, id, data);
  }

  async deleteSystemContext(id: string): Promise<void> {
    return configsApi.deleteSystemContext(this.requestFn, id);
  }

  async activateSystemContext(id: string, userId: string): Promise<any> {
    return configsApi.activateSystemContext(this.requestFn, id, userId);
  }

  async generateSystemContextDraft(data: {
    user_id: string;
    scene: string;
    style?: string;
    language?: string;
    output_format?: string;
    constraints?: string[];
    forbidden?: string[];
    candidate_count?: number;
    ai_model_config?: any;
  }): Promise<any> {
    return configsApi.generateSystemContextDraft(this.requestFn, data);
  }

  async optimizeSystemContextDraft(data: {
    user_id: string;
    content: string;
    goal?: string;
    keep_intent?: boolean;
    ai_model_config?: any;
  }): Promise<any> {
    return configsApi.optimizeSystemContextDraft(this.requestFn, data);
  }

  async evaluateSystemContextDraft(data: {
    content: string;
  }): Promise<any> {
    return configsApi.evaluateSystemContextDraft(this.requestFn, data);
  }

  // 应用（Application）相关API
  async getApplications(userId?: string): Promise<any[]> {
    return configsApi.getApplications(this.requestFn, userId);
  }

  async getApplication(id: string): Promise<any> {
    return configsApi.getApplication(this.requestFn, id);
  }

  async createApplication(data: {
    name: string;
    url: string;
    icon_url?: string | null;
    user_id?: string;
  }): Promise<any> {
    return configsApi.createApplication(this.requestFn, data);
  }

  async updateApplication(id: string, data: {
    name?: string;
    url?: string;
    icon_url?: string | null;
  }): Promise<any> {
    return configsApi.updateApplication(this.requestFn, id, data);
  }

  async deleteApplication(id: string): Promise<any> {
    return configsApi.deleteApplication(this.requestFn, id);
  }

  // 智能体（Agent）相关API
  async getAgents(userId?: string): Promise<any[]> {
    return configsApi.getAgents(this.requestFn, userId);
  }

  async createAgent(data: {
    name: string;
    description?: string;
    ai_model_config_id: string;
    mcp_config_ids?: string[];
    callable_agent_ids?: string[];
    system_context_id?: string;
    project_id?: string | null;
    workspace_dir?: string | null;
    user_id?: string;
    enabled?: boolean;
    app_ids?: string[];
  }): Promise<any> {
    return configsApi.createAgent(this.requestFn, data);
  }

  async updateAgent(agentId: string, data: {
    name?: string;
    description?: string;
    ai_model_config_id?: string;
    mcp_config_ids?: string[];
    callable_agent_ids?: string[];
    system_context_id?: string;
    project_id?: string | null;
    workspace_dir?: string | null;
    enabled?: boolean;
    app_ids?: string[];
  }): Promise<any> {
    return configsApi.updateAgent(this.requestFn, agentId, data);
  }

  async deleteAgent(agentId: string): Promise<any> {
    return configsApi.deleteAgent(this.requestFn, agentId);
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
      const query = buildQuery({ limit: params.limit, offset: params.offset });
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
  async streamChat(
    sessionId: string,
    content: string,
    modelConfig: any,
    userId?: string,
    attachments?: any[],
    reasoningEnabled?: boolean,
    options?: { turnId?: string }
  ): Promise<ReadableStream> {
    const useResponses = modelConfig?.supports_responses === true;
    const url = `${this.baseUrl}/${useResponses ? 'agent_v3' : 'agent_v2'}/chat/stream`;
    
    const response = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.accessToken ? { Authorization: `Bearer ${this.accessToken}` } : {}),
      },
      body: JSON.stringify({
        session_id: sessionId,
        content: content,
        user_id: userId,
        attachments: attachments || [],
        reasoning_enabled: reasoningEnabled,
        turn_id: options?.turnId,
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
    this.applyRefreshedAccessToken(response);

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
    options?: { useResponses?: boolean; turnId?: string }
  ): Promise<ReadableStream> {
    const useResponses = options?.useResponses === true;
    const url = `${this.baseUrl}/${useResponses ? 'agent_v3/agents' : 'agents'}/chat/stream`;

    const response = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Accept': 'text/event-stream',
        ...(this.accessToken ? { Authorization: `Bearer ${this.accessToken}` } : {}),
      },
      body: JSON.stringify({
        session_id: sessionId,
        content: content,
        agent_id: agentId,
        user_id: userId,
        attachments: attachments || [],
        reasoning_enabled: reasoningEnabled,
        turn_id: options?.turnId,
      }),
    });
    this.applyRefreshedAccessToken(response);

    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }

    if (!response.body) {
      throw new Error('Response body is null');
    }

    return response.body;
  }

  async getTaskManagerTasks(
    sessionId: string,
    options?: { conversationTurnId?: string; includeDone?: boolean; limit?: number }
  ): Promise<any[]> {
    if (!sessionId) {
      return [];
    }

    const params = new URLSearchParams();
    params.set('session_id', sessionId);
    if (options?.conversationTurnId) {
      params.set('conversation_turn_id', options.conversationTurnId);
    }
    if (options?.includeDone === true) {
      params.set('include_done', 'true');
    }
    if (typeof options?.limit === 'number') {
      params.set('limit', String(options.limit));
    }

    const result = await this.request<any>('/task-manager/tasks?' + params.toString());
    if (Array.isArray(result)) {
      return result;
    }
    return Array.isArray(result?.tasks) ? result.tasks : [];
  }

  async updateTaskManagerTask(
    sessionId: string,
    taskId: string,
    payload: {
      title?: string;
      details?: string;
      priority?: 'high' | 'medium' | 'low';
      status?: 'todo' | 'doing' | 'blocked' | 'done';
      tags?: string[];
      due_at?: string | null;
    }
  ): Promise<any> {
    if (!sessionId) {
      throw new Error('sessionId is required');
    }
    if (!taskId) {
      throw new Error('taskId is required');
    }

    const params = new URLSearchParams();
    params.set('session_id', sessionId);
    return this.request<any>('/task-manager/tasks/' + encodeURIComponent(taskId) + '?' + params.toString(), {
      method: 'PATCH',
      body: JSON.stringify(payload),
    });
  }

  async completeTaskManagerTask(sessionId: string, taskId: string): Promise<any> {
    if (!sessionId) {
      throw new Error('sessionId is required');
    }
    if (!taskId) {
      throw new Error('taskId is required');
    }

    const params = new URLSearchParams();
    params.set('session_id', sessionId);
    return this.request<any>('/task-manager/tasks/' + encodeURIComponent(taskId) + '/complete?' + params.toString(), {
      method: 'POST',
      body: JSON.stringify({}),
    });
  }

  async deleteTaskManagerTask(sessionId: string, taskId: string): Promise<any> {
    if (!sessionId) {
      throw new Error('sessionId is required');
    }
    if (!taskId) {
      throw new Error('taskId is required');
    }

    const params = new URLSearchParams();
    params.set('session_id', sessionId);
    return this.request<any>('/task-manager/tasks/' + encodeURIComponent(taskId) + '?' + params.toString(), {
      method: 'DELETE',
    });
  }

  async submitTaskReviewDecision(
    reviewId: string,
    payload: { action: 'confirm' | 'cancel'; tasks?: any[]; reason?: string }
  ): Promise<any> {
    if (!reviewId) {
      throw new Error('reviewId is required');
    }

    return this.request<any>(`/task-manager/reviews/${encodeURIComponent(reviewId)}/decision`, {
      method: 'POST',
      body: JSON.stringify(payload),
    });
  }

  async getPendingUiPrompts(
    sessionId: string,
    options?: { limit?: number }
  ): Promise<any[]> {
    if (!sessionId) {
      return [];
    }

    const params = new URLSearchParams();
    params.set('session_id', sessionId);
    if (typeof options?.limit === 'number') {
      params.set('limit', String(options.limit));
    }

    const result = await this.request<any>('/ui-prompts/pending?' + params.toString());
    if (Array.isArray(result)) {
      return result;
    }
    return Array.isArray(result?.prompts) ? result.prompts : [];
  }

  async getUiPromptHistory(
    sessionId: string,
    options?: { limit?: number; includePending?: boolean }
  ): Promise<any[]> {
    if (!sessionId) {
      return [];
    }

    const params = new URLSearchParams();
    params.set('session_id', sessionId);
    if (typeof options?.limit === 'number') {
      params.set('limit', String(options.limit));
    }
    if (options?.includePending === true) {
      params.set('include_pending', 'true');
    }

    const result = await this.request<any>('/ui-prompts/history?' + params.toString());
    if (Array.isArray(result)) {
      return result;
    }
    return Array.isArray(result?.prompts) ? result.prompts : [];
  }

  async submitUiPromptResponse(
    promptId: string,
    payload: {
      status: 'ok' | 'canceled' | 'timeout';
      values?: Record<string, string>;
      selection?: string | string[];
      reason?: string;
    }
  ): Promise<any> {
    if (!promptId) {
      throw new Error('promptId is required');
    }

    return this.request<any>(`/ui-prompts/${encodeURIComponent(promptId)}/respond`, {
      method: 'POST',
      body: JSON.stringify(payload),
    });
  }

  async notepadInit(): Promise<any> {
    return this.request<any>('/notepad/init');
  }

  async listNotepadFolders(): Promise<any> {
    return this.request<any>('/notepad/folders');
  }

  async createNotepadFolder(payload: { folder: string }): Promise<any> {
    return this.request<any>('/notepad/folders', {
      method: 'POST',
      body: JSON.stringify(payload),
    });
  }

  async renameNotepadFolder(payload: { from: string; to: string }): Promise<any> {
    return this.request<any>('/notepad/folders', {
      method: 'PATCH',
      body: JSON.stringify(payload),
    });
  }

  async deleteNotepadFolder(options: { folder: string; recursive?: boolean }): Promise<any> {
    const params = new URLSearchParams();
    params.set('folder', options.folder);
    if (options.recursive === true) {
      params.set('recursive', 'true');
    }
    return this.request<any>('/notepad/folders?' + params.toString(), {
      method: 'DELETE',
    });
  }

  async listNotepadNotes(options?: {
    folder?: string;
    recursive?: boolean;
    tags?: string[];
    match?: 'all' | 'any';
    query?: string;
    limit?: number;
  }): Promise<any> {
    const params = new URLSearchParams();
    if (options?.folder) {
      params.set('folder', options.folder);
    }
    if (typeof options?.recursive === 'boolean') {
      params.set('recursive', options.recursive ? 'true' : 'false');
    }
    if (options?.tags && options.tags.length > 0) {
      params.set('tags', options.tags.join(','));
    }
    if (options?.match) {
      params.set('match', options.match);
    }
    if (options?.query) {
      params.set('query', options.query);
    }
    if (typeof options?.limit === 'number') {
      params.set('limit', String(options.limit));
    }
    const query = params.toString();
    return this.request<any>(`/notepad/notes${query ? `?${query}` : ''}`);
  }

  async createNotepadNote(payload: {
    folder?: string;
    title?: string;
    content?: string;
    tags?: string[];
  }): Promise<any> {
    return this.request<any>('/notepad/notes', {
      method: 'POST',
      body: JSON.stringify(payload),
    });
  }

  async getNotepadNote(noteId: string): Promise<any> {
    return this.request<any>(`/notepad/notes/${encodeURIComponent(noteId)}`);
  }

  async updateNotepadNote(noteId: string, payload: {
    title?: string;
    content?: string;
    folder?: string;
    tags?: string[];
  }): Promise<any> {
    return this.request<any>(`/notepad/notes/${encodeURIComponent(noteId)}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    });
  }

  async deleteNotepadNote(noteId: string): Promise<any> {
    return this.request<any>(`/notepad/notes/${encodeURIComponent(noteId)}`, {
      method: 'DELETE',
    });
  }

  async listNotepadTags(): Promise<any> {
    return this.request<any>('/notepad/tags');
  }

  async searchNotepadNotes(options: {
    query: string;
    folder?: string;
    recursive?: boolean;
    tags?: string[];
    match?: 'all' | 'any';
    include_content?: boolean;
    limit?: number;
  }): Promise<any> {
    const params = new URLSearchParams();
    params.set('query', options.query);
    if (options.folder) {
      params.set('folder', options.folder);
    }
    if (typeof options.recursive === 'boolean') {
      params.set('recursive', options.recursive ? 'true' : 'false');
    }
    if (options.tags && options.tags.length > 0) {
      params.set('tags', options.tags.join(','));
    }
    if (options.match) {
      params.set('match', options.match);
    }
    if (typeof options.include_content === 'boolean') {
      params.set('include_content', options.include_content ? 'true' : 'false');
    }
    if (typeof options.limit === 'number') {
      params.set('limit', String(options.limit));
    }
    return this.request<any>('/notepad/search?' + params.toString());
  }

  async getSessionSummaryJobConfig(userId?: string): Promise<any> {
    const params = userId ? `?user_id=${encodeURIComponent(userId)}` : '';
    return this.request<any>(`/session-summary-job-config${params}`);
  }

  async updateSessionSummaryJobConfig(payload: {
    user_id?: string;
    enabled?: boolean;
    summary_model_config_id?: string | null;
    token_limit?: number;
    message_count_limit?: number;
    round_limit?: number;
    target_summary_tokens?: number;
    job_interval_seconds?: number;
  }): Promise<any> {
    return this.request<any>('/session-summary-job-config', {
      method: 'PUT',
      body: JSON.stringify(payload),
    });
  }

  async patchSessionSummaryJobConfig(payload: {
    user_id?: string;
    enabled?: boolean;
    summary_model_config_id?: string | null;
    token_limit?: number;
    message_count_limit?: number;
    round_limit?: number;
    target_summary_tokens?: number;
    job_interval_seconds?: number;
  }): Promise<any> {
    return this.request<any>('/session-summary-job-config', {
      method: 'PATCH',
      body: JSON.stringify(payload),
    });
  }

  async getSessionSummaries(
    sessionId: string,
    options?: { limit?: number; offset?: number }
  ): Promise<{ items: any[]; total: number; has_summary: boolean }> {
    if (!sessionId) {
      return { items: [], total: 0, has_summary: false };
    }

    const params = new URLSearchParams();
    if (typeof options?.limit === 'number') {
      params.set('limit', String(options.limit));
    }
    if (typeof options?.offset === 'number') {
      params.set('offset', String(options.offset));
    }
    const query = params.toString();
    const result = await this.request<any>(
      `/sessions/${encodeURIComponent(sessionId)}/summaries${query ? `?${query}` : ''}`
    );

    return {
      items: Array.isArray(result?.items) ? result.items : [],
      total: typeof result?.total === 'number' ? result.total : 0,
      has_summary: result?.has_summary === true,
    };
  }

  async deleteSessionSummary(sessionId: string, summaryId: string): Promise<any> {
    if (!sessionId) {
      throw new Error('sessionId is required');
    }
    if (!summaryId) {
      throw new Error('summaryId is required');
    }

    return this.request<any>(
      `/sessions/${encodeURIComponent(sessionId)}/summaries/${encodeURIComponent(summaryId)}`,
      { method: 'DELETE' }
    );
  }

  async clearSessionSummaries(sessionId: string): Promise<any> {
    if (!sessionId) {
      throw new Error('sessionId is required');
    }

    return this.request<any>(`/sessions/${encodeURIComponent(sessionId)}/summaries`, {
      method: 'DELETE',
    });
  }

  async register(data: {
    email: string;
    password: string;
    display_name?: string;
  }): Promise<any> {
    return this.request<any>('/auth/register', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async login(data: { email: string; password: string }): Promise<any> {
    return this.request<any>('/auth/login', {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async getMe(): Promise<any> {
    return this.request<any>('/auth/me');
  }

  // 停止聊天流
  async stopChat(sessionId: string, options?: { useResponses?: boolean }): Promise<any> {
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
    const qs = userId ? `?user_id=${encodeURIComponent(userId)}` : '';
    return this.request<any>(`/user-settings${qs}`);
  }

  async updateUserSettings(userId: string, settings: Record<string, any>): Promise<any> {
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
