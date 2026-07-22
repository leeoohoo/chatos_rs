// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { ApiRequestError, buildParsedJsonErrorPayload } from '../client/shared';
import type {
  LocalRuntimeBridgeRequest,
  LocalRuntimeBridgeResponse,
} from './types';

declare global {
  interface Window {
    chatosLocalRuntime?: {
      apiRequest: (request: LocalRuntimeBridgeRequest) => Promise<LocalRuntimeBridgeResponse>;
      authenticateDesktopTicket?: (ticket: string) => Promise<unknown>;
    };
  }
}

const LOCAL_RUNTIME_AUTH_READY_ATTEMPTS = 50;
const LOCAL_RUNTIME_AUTH_READY_DELAY_MS = 100;

const delay = (milliseconds: number): Promise<void> => (
  new Promise((resolve) => window.setTimeout(resolve, milliseconds))
);

export const localRuntimeBridgeAvailable = (): boolean =>
  typeof window !== 'undefined'
  && typeof window.chatosLocalRuntime?.apiRequest === 'function';

export const requestLocalRuntime = async <T>(
  endpoint: string,
  options: RequestInit = {},
): Promise<T> => {
  const bridge = typeof window !== 'undefined' ? window.chatosLocalRuntime : undefined;
  if (!bridge?.apiRequest) {
    throw new ApiRequestError('该本地项目需要在 Chat OS 桌面客户端中打开', {
      status: 409,
      code: 'local_runtime_unavailable',
    });
  }

  const headers = new Headers(options.headers || {});
  if (options.body != null && !headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }
  for (let attempt = 0; attempt < LOCAL_RUNTIME_AUTH_READY_ATTEMPTS; attempt += 1) {
    const response = await bridge.apiRequest({
      endpoint,
      method: options.method || 'GET',
      headers: Object.fromEntries(headers.entries()),
      body: typeof options.body === 'string' ? options.body : null,
    });
    const raw = response.body || '';
    if (!response.ok) {
      const parsed = buildParsedJsonErrorPayload(raw, '本地运行时请求失败');
      const waitingForAuthentication = response.status === 409
        && parsed.code === 'local_runtime_not_authenticated'
        && attempt + 1 < LOCAL_RUNTIME_AUTH_READY_ATTEMPTS;
      if (waitingForAuthentication) {
        await delay(LOCAL_RUNTIME_AUTH_READY_DELAY_MS);
        continue;
      }
      throw new ApiRequestError(parsed.message, {
        status: response.status,
        code: parsed.code,
        payload: parsed.payload,
      });
    }
    if (!raw) {
      return {} as T;
    }
    return JSON.parse(raw) as T;
  }
  throw new ApiRequestError('Local Connector 登录尚未完成', {
    status: 409,
    code: 'local_runtime_not_authenticated',
  });
};
