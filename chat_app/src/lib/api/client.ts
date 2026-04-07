// API客户端，用于连接后端服务
import * as fsApi from './client/fs';
import { configFacade, type ConfigFacade } from './client/facades/configFacade';
import { runtimeFacade, type RuntimeFacade } from './client/facades/runtimeFacade';
import { workspaceFacade, type WorkspaceFacade } from './client/facades/workspaceFacade';
import { ApiRequestError } from './client/shared';
import type {
  ConversationMessagePayload,
} from './client/types';
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

  getRequestFn(): workspaceApi.ApiRequestFn {
    return this.requestFn;
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
  getBinaryApiContext(): fsApi.BinaryApiContext {
    return {
      baseUrl: this.baseUrl,
      accessToken: this.accessToken,
      applyRefreshedAccessToken: (response: Response) => this.applyRefreshedAccessToken(response),
    };
  }
}

interface ApiClient extends WorkspaceFacade, ConfigFacade, RuntimeFacade {}

Object.assign(
  ApiClient.prototype,
  workspaceFacade,
  configFacade,
  runtimeFacade,
);

// 导出单例实例
export const apiClient = new ApiClient();

// 为了保持向后兼容性，导出conversationsApi对象
export const conversationsApi = {
  getDetails: (conversationId: string) => apiClient.getConversationDetails(conversationId),
  getAssistant: (conversationId: string) => apiClient.getAssistant(conversationId),
  getMcpServers: (conversationId?: string) => apiClient.getMcpServers(conversationId),
  saveMessage: (conversationId: string, message: ConversationMessagePayload) => apiClient.saveMessage(conversationId, message),
  getMessages: (conversationId: string, params?: { limit?: number; offset?: number }) => apiClient.getMessages(conversationId, params),
  addMessage: (conversationId: string, message: ConversationMessagePayload) => apiClient.addMessage(conversationId, message),
};

export default ApiClient;
