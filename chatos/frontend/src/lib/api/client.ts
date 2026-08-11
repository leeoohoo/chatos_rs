// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// API客户端，用于连接后端服务
import * as fsApi from './client/fs';
import { configFacade, type ConfigFacade } from './client/facades/configFacade';
import { runtimeFacade, type RuntimeFacade } from './client/facades/runtimeFacade';
import { workspaceFacade, type WorkspaceFacade } from './client/facades/workspaceFacade';
import {
  ApiRequestError,
  buildParsedJsonErrorPayload,
  parseJsonTextSafely,
} from './client/shared';
import { applyClientSurfaceHeader } from './client/surface';
import * as streamApi from './client/stream';
import type {
  ConversationMessagePayload,
  WebSocketTicketResponse,
} from './client/types';
import * as workspaceApi from './client/workspace';

// 统一通过 ChatOS 网关入口访问 API，便于桌面端、浏览器端与部署环境保持一致。
// 可通过 VITE_API_BASE_URL 显式覆盖（例如 https://your.domain/api/chatos）。
const ENV_API_BASE_URL = (import.meta.env.VITE_API_BASE_URL || '').trim();
const API_BASE_URL = ENV_API_BASE_URL || '/api/chatos';

class ApiClient {
  private baseUrl: string;
  private accessToken: string | null = null;
  private tokenRefreshListeners = new Set<(token: string) => void>();
  private authenticationFailureListeners = new Set<() => void>();
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

  async issueWebSocketTicket(): Promise<string> {
    const response = await this.request<WebSocketTicketResponse>('/auth/ws-ticket', {
      method: 'POST',
    });
    const ticket = String(response?.ticket || '').trim();
    if (!ticket) {
      throw new Error('签发 WebSocket 连接票据失败');
    }
    return ticket;
  }

  onAccessTokenRefresh(listener: (token: string) => void): () => void {
    this.tokenRefreshListeners.add(listener);
    return () => this.tokenRefreshListeners.delete(listener);
  }

  onAuthenticationFailure(listener: () => void): () => void {
    this.authenticationFailureListeners.add(listener);
    return () => this.authenticationFailureListeners.delete(listener);
  }

  private invalidateAccessToken(): void {
    if (!this.accessToken) {
      return;
    }
    this.accessToken = null;
    this.authenticationFailureListeners.forEach((listener) => {
      try {
        listener();
      } catch (error) {
        console.error('Authentication failure listener failed:', error);
      }
    });
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
    const bodyIsFormData = typeof FormData !== 'undefined' && options.body instanceof FormData;
    if (!headers.has('Content-Type') && !bodyIsFormData) {
      headers.set('Content-Type', 'application/json');
    }
    if (this.accessToken && !headers.has('Authorization')) {
      headers.set('Authorization', `Bearer ${this.accessToken}`);
    }
    applyClientSurfaceHeader(headers);
    const config: RequestInit = {
      ...options,
      headers,
    };

    try {
      const response = await fetch(url, config);
      this.applyRefreshedAccessToken(response);
      if (response.status === 401) {
        this.invalidateAccessToken();
      }
      const text = await response.text();
      let parsedBody: unknown = null;

      if (text) {
        const parsedResult = parseJsonTextSafely(text);
        if (parsedResult.ok) {
          parsedBody = parsedResult.parsed;
        } else if (response.ok) {
          const parseError = new Error(`Invalid JSON response: ${text}`);
          if (response.ok) {
            console.error(`JSON parse error for ${endpoint}:`, parseError, 'Response text:', text);
            throw parseError;
          }
        }
      }

      if (!response.ok) {
        const {
          message: errorMessage,
          code: errorCode,
          payload,
        } = buildParsedJsonErrorPayload(text, `HTTP error! status: ${response.status}`);
        throw new ApiRequestError(errorMessage, {
          status: response.status,
          code: errorCode,
          payload,
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

  getStreamApiContext(): streamApi.StreamApiContext {
    return {
      baseUrl: this.baseUrl,
      accessToken: this.accessToken,
      applyRefreshedAccessToken: (response: Response) => this.applyRefreshedAccessToken(response),
    };
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
  saveMessage: (conversationId: string, message: ConversationMessagePayload) => apiClient.saveMessage(conversationId, message),
  getMessages: (conversationId: string, params?: { limit?: number; offset?: number }) => apiClient.getMessages(conversationId, params),
  addMessage: (conversationId: string, message: ConversationMessagePayload) => apiClient.addMessage(conversationId, message),
};

export default ApiClient;
