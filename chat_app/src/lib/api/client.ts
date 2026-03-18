// API客户端，用于连接后端服务
import * as accountApi from './client/account';
import * as conversationApi from './client/conversation';
import * as configsApi from './client/configs';
import * as notepadApi from './client/notepad';
import {
  ApiRequestError,
  buildQuery,
  guessFilenameFromPath,
  parseFilenameFromContentDisposition,
} from './client/shared';
import * as streamApi from './client/stream';
import * as summaryApi from './client/summary';
import * as tasksApi from './client/tasks';
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
        const errorCode = typeof parsedBody?.code === 'string' ? parsedBody.code : undefined;
        const errorMessage =
          (typeof parsedBody?.error === 'string' && parsedBody.error) ||
          (typeof parsedBody?.message === 'string' && parsedBody.message) ||
          `HTTP error! status: ${response.status}`;
        throw new ApiRequestError(errorMessage, {
          status: response.status,
          code: errorCode,
          payload: parsedBody,
        });
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

  private getStreamContext(): streamApi.StreamApiContext {
    return {
      baseUrl: this.baseUrl,
      accessToken: this.accessToken,
      applyRefreshedAccessToken: (response: Response) => this.applyRefreshedAccessToken(response),
    };
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

  async getContacts(
    userId?: string,
    paging?: { limit?: number; offset?: number },
  ): Promise<any[]> {
    return workspaceApi.getContacts(this.requestFn, userId, paging);
  }

  async createContact(data: { agent_id: string; agent_name_snapshot?: string; user_id?: string }): Promise<any> {
    return workspaceApi.createContact(this.requestFn, data);
  }

  async deleteContact(contactId: string): Promise<any> {
    return workspaceApi.deleteContact(this.requestFn, contactId);
  }

  async getContactProjectMemories(
    contactId: string,
    projectId: string,
    paging?: { limit?: number; offset?: number },
  ): Promise<any[]> {
    return workspaceApi.getContactProjectMemories(this.requestFn, contactId, projectId, paging);
  }

  async getContactProjects(
    contactId: string,
    paging?: { limit?: number; offset?: number },
  ): Promise<any[]> {
    return workspaceApi.getContactProjects(this.requestFn, contactId, paging);
  }

  async getContactAgentRecalls(
    contactId: string,
    paging?: { limit?: number; offset?: number },
  ): Promise<any[]> {
    return workspaceApi.getContactAgentRecalls(this.requestFn, contactId, paging);
  }

  async getSessionMessages(
    sessionId: string,
    params?: { limit?: number; offset?: number; compact?: boolean; strategy?: string },
  ): Promise<any[]> {
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

  async listProjectContacts(
    projectId: string,
    paging?: { limit?: number; offset?: number },
  ): Promise<any[]> {
    return workspaceApi.listProjectContacts(this.requestFn, projectId, paging);
  }

  async addProjectContact(
    projectId: string,
    data: { contact_id: string },
  ): Promise<any> {
    return workspaceApi.addProjectContact(this.requestFn, projectId, data);
  }

  async removeProjectContact(
    projectId: string,
    contactId: string,
  ): Promise<any> {
    return workspaceApi.removeProjectContact(this.requestFn, projectId, contactId);
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
    params?: { limit?: number; offset?: number; before?: string }
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
      let code: string | undefined;
      let payload: any = null;
      if (text) {
        try {
          const parsed = JSON.parse(text);
          payload = parsed;
          code = typeof parsed?.code === 'string' ? parsed.code : undefined;
          message =
            (typeof parsed?.error === 'string' && parsed.error) ||
            (typeof parsed?.message === 'string' && parsed.message) ||
            message;
        } catch {
          message = text;
        }
      }
      throw new ApiRequestError(message, {
        status: response.status,
        code,
        payload,
      });
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

  async getMemoryAgents(
    userId?: string,
    options?: { enabled?: boolean; limit?: number; offset?: number },
  ): Promise<any[]> {
    const query = buildQuery({
      user_id: userId,
      enabled: typeof options?.enabled === 'boolean' ? options.enabled : undefined,
      limit: options?.limit,
      offset: options?.offset,
    });
    return this.request<any[]>(`/memory-agents${query}`);
  }

  async getMemoryAgentRuntimeContext(agentId: string): Promise<any> {
    return this.request<any>(`/memory-agents/${encodeURIComponent(agentId)}/runtime-context`);
  }

  // 会话详情和助手相关API (从index.ts合并)
  async getConversationDetails(conversationId: string) {
    return conversationApi.getConversationDetails(this.requestFn, conversationId);
  }

  async getAssistant(_conversationId: string) {
    return conversationApi.getAssistant(this.requestFn, _conversationId);
  }

  async getMcpServers(_conversationId?: string) {
    return conversationApi.getMcpServers(this.requestFn, _conversationId);
  }

  async getMcpConfigResource(configId: string): Promise<{ success: boolean; config: any; alias?: string }> {
    return conversationApi.getMcpConfigResource(this.requestFn, configId);
  }

  async getMcpConfigResourceByCommand(data: {
    type: 'stdio' | 'http';
    command: string;
    args?: string[] | null;
    env?: Record<string, string> | null;
    cwd?: string | null;
    alias?: string | null;
  }): Promise<{ success: boolean; config: any; alias?: string }> {
    return conversationApi.getMcpConfigResourceByCommand(this.requestFn, data);
  }

  async saveMessage(conversationId: string, message: any) {
    return conversationApi.saveMessage(this.requestFn, conversationId, message);
  }

  async getMessages(conversationId: string, params: { limit?: number; offset?: number } = {}) {
    return conversationApi.getMessages(this.requestFn, conversationId, params);
  }

  async addMessage(conversationId: string, message: any) {
    return conversationApi.addMessage(this.requestFn, conversationId, message);
  }

  // 流式聊天接口
  async streamChat(
    sessionId: string,
    content: string,
    modelConfig: any,
    userId?: string,
    attachments?: any[],
    reasoningEnabled?: boolean,
    options?: {
      turnId?: string;
      contactAgentId?: string | null;
      projectId?: string | null;
      projectRoot?: string | null;
      mcpEnabled?: boolean;
      enabledMcpIds?: string[];
    }
  ): Promise<ReadableStream> {
    return streamApi.streamChat(
      this.getStreamContext(),
      sessionId,
      content,
      modelConfig,
      userId,
      attachments,
      reasoningEnabled,
      options
    );
  }

  async getTaskManagerTasks(
    sessionId: string,
    options?: { conversationTurnId?: string; includeDone?: boolean; limit?: number }
  ): Promise<any[]> {
    return tasksApi.getTaskManagerTasks(this.requestFn, sessionId, options);
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
    return tasksApi.updateTaskManagerTask(this.requestFn, sessionId, taskId, payload);
  }

  async completeTaskManagerTask(sessionId: string, taskId: string): Promise<any> {
    return tasksApi.completeTaskManagerTask(this.requestFn, sessionId, taskId);
  }

  async deleteTaskManagerTask(sessionId: string, taskId: string): Promise<any> {
    return tasksApi.deleteTaskManagerTask(this.requestFn, sessionId, taskId);
  }

  async submitTaskReviewDecision(
    reviewId: string,
    payload: { action: 'confirm' | 'cancel'; tasks?: any[]; reason?: string }
  ): Promise<any> {
    return tasksApi.submitTaskReviewDecision(this.requestFn, reviewId, payload);
  }

  async getPendingUiPrompts(
    sessionId: string,
    options?: { limit?: number }
  ): Promise<any[]> {
    return tasksApi.getPendingUiPrompts(this.requestFn, sessionId, options);
  }

  async getUiPromptHistory(
    sessionId: string,
    options?: { limit?: number; includePending?: boolean }
  ): Promise<any[]> {
    return tasksApi.getUiPromptHistory(this.requestFn, sessionId, options);
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
    return tasksApi.submitUiPromptResponse(this.requestFn, promptId, payload);
  }

  async notepadInit(): Promise<any> {
    return notepadApi.notepadInit(this.requestFn);
  }

  async listNotepadFolders(): Promise<any> {
    return notepadApi.listNotepadFolders(this.requestFn);
  }

  async createNotepadFolder(payload: { folder: string }): Promise<any> {
    return notepadApi.createNotepadFolder(this.requestFn, payload);
  }

  async renameNotepadFolder(payload: { from: string; to: string }): Promise<any> {
    return notepadApi.renameNotepadFolder(this.requestFn, payload);
  }

  async deleteNotepadFolder(options: { folder: string; recursive?: boolean }): Promise<any> {
    return notepadApi.deleteNotepadFolder(this.requestFn, options);
  }

  async listNotepadNotes(options?: {
    folder?: string;
    recursive?: boolean;
    tags?: string[];
    match?: 'all' | 'any';
    query?: string;
    limit?: number;
  }): Promise<any> {
    return notepadApi.listNotepadNotes(this.requestFn, options);
  }

  async createNotepadNote(payload: {
    folder?: string;
    title?: string;
    content?: string;
    tags?: string[];
  }): Promise<any> {
    return notepadApi.createNotepadNote(this.requestFn, payload);
  }

  async getNotepadNote(noteId: string): Promise<any> {
    return notepadApi.getNotepadNote(this.requestFn, noteId);
  }

  async updateNotepadNote(noteId: string, payload: {
    title?: string;
    content?: string;
    folder?: string;
    tags?: string[];
  }): Promise<any> {
    return notepadApi.updateNotepadNote(this.requestFn, noteId, payload);
  }

  async deleteNotepadNote(noteId: string): Promise<any> {
    return notepadApi.deleteNotepadNote(this.requestFn, noteId);
  }

  async listNotepadTags(): Promise<any> {
    return notepadApi.listNotepadTags(this.requestFn);
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
    return notepadApi.searchNotepadNotes(this.requestFn, options);
  }

  async getSessionSummaryJobConfig(userId?: string): Promise<any> {
    return summaryApi.getSessionSummaryJobConfig(this.requestFn, userId);
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
    return summaryApi.updateSessionSummaryJobConfig(this.requestFn, payload);
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
    return summaryApi.patchSessionSummaryJobConfig(this.requestFn, payload);
  }

  async getSessionSummaries(
    sessionId: string,
    options?: { limit?: number; offset?: number }
  ): Promise<{ items: any[]; total: number; has_summary: boolean }> {
    return summaryApi.getSessionSummaries(this.requestFn, sessionId, options);
  }

  async deleteSessionSummary(sessionId: string, summaryId: string): Promise<any> {
    return summaryApi.deleteSessionSummary(this.requestFn, sessionId, summaryId);
  }

  async clearSessionSummaries(sessionId: string): Promise<any> {
    return summaryApi.clearSessionSummaries(this.requestFn, sessionId);
  }

  async register(data: {
    username: string;
    password: string;
  }): Promise<any> {
    return accountApi.register(this.requestFn, data);
  }

  async login(data: { username: string; password: string }): Promise<any> {
    return accountApi.login(this.requestFn, data);
  }

  async getMe(): Promise<any> {
    return accountApi.getMe(this.requestFn);
  }

  // 停止聊天流
  async stopChat(sessionId: string, options?: { useResponses?: boolean }): Promise<any> {
    return streamApi.stopChat(this.requestFn, sessionId, options);
  }

  // User settings APIs
  async getUserSettings(userId?: string): Promise<any> {
    return accountApi.getUserSettings(this.requestFn, userId);
  }

  async updateUserSettings(userId: string, settings: Record<string, any>): Promise<any> {
    return accountApi.updateUserSettings(this.requestFn, userId, settings);
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
