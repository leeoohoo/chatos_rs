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
    };
  }
}

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
  const response = await bridge.apiRequest({
    endpoint,
    method: options.method || 'GET',
    headers: Object.fromEntries(headers.entries()),
    body: typeof options.body === 'string' ? options.body : null,
  });
  const raw = response.body || '';
  if (!response.ok) {
    const parsed = buildParsedJsonErrorPayload(raw, '本地运行时请求失败');
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
};
