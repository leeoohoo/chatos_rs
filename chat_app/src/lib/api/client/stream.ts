import type {
  RuntimeGuidanceSubmitPayload,
  RuntimeGuidanceSubmitResponse,
  StopChatResponse,
  StreamChatAttachmentPayload,
  StreamChatModelConfigPayload,
  StreamChatOptions,
} from './types';
import type { ApiRequestFn } from './workspace';

export interface StreamApiContext {
  baseUrl: string;
  accessToken: string | null;
  applyRefreshedAccessToken: (response: Response) => void;
}

const buildStreamHttpError = async (response: Response): Promise<Error> => {
  const status = response.status;
  const raw = await response.text().catch(() => '');
  if (!raw) {
    return new Error(`HTTP ${status}: 请求失败`);
  }

  try {
    const payload = JSON.parse(raw);
    const code = typeof payload?.code === 'string' ? payload.code : '';
    const message = payload?.error || payload?.message || raw;
    if (typeof message === 'string' && message.trim().length > 0) {
      if (code) {
        return new Error(`[${code}] HTTP ${status}: ${message.trim()}`);
      }
      return new Error(`HTTP ${status}: ${message.trim()}`);
    }
  } catch {
    // ignore JSON parse error and fallback to raw text
  }

  const compact = raw.trim().length > 0 ? raw.trim() : '请求失败';
  return new Error(`HTTP ${status}: ${compact}`);
};

export const streamChat = async (
  context: StreamApiContext,
  conversationId: string,
  content: string,
  modelConfig: StreamChatModelConfigPayload,
  userId?: string,
  attachments?: StreamChatAttachmentPayload[],
  reasoningEnabled?: boolean,
  options?: StreamChatOptions,
): Promise<ReadableStream> => {
  const useResponses = modelConfig?.supports_responses === true;
  const url = `${context.baseUrl}/${useResponses ? 'agent_v3' : 'agent_v2'}/chat/stream`;
  const hasRemoteConnectionId = Boolean(
    options && Object.prototype.hasOwnProperty.call(options, 'remoteConnectionId'),
  );

  const response = await fetch(url, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...(context.accessToken ? { Authorization: `Bearer ${context.accessToken}` } : {}),
    },
    body: JSON.stringify({
      conversation_id: conversationId,
      content,
      user_id: userId,
      attachments: attachments || [],
      reasoning_enabled: reasoningEnabled,
      turn_id: options?.turnId,
      contact_agent_id: options?.contactAgentId || undefined,
      remote_connection_id: hasRemoteConnectionId
        ? (options?.remoteConnectionId ?? null)
        : undefined,
      project_id: options?.projectId || undefined,
      project_root: options?.projectRoot || undefined,
      mcp_enabled: options?.mcpEnabled,
      enabled_mcp_ids: options?.enabledMcpIds || [],
      ai_model_config: {
        provider: modelConfig.provider,
        model_name: modelConfig.model_name,
        temperature: modelConfig.temperature || 0.7,
        thinking_level: modelConfig.thinking_level,
        api_key: modelConfig.api_key,
        base_url: modelConfig.base_url,
        supports_images: modelConfig.supports_images === true,
        supports_reasoning: modelConfig.supports_reasoning === true,
        supports_responses: modelConfig.supports_responses === true,
      },
    }),
  });
  context.applyRefreshedAccessToken(response);

  if (!response.ok) {
    throw await buildStreamHttpError(response);
  }

  if (!response.body) {
    throw new Error('Response body is null');
  }

  return response.body;
};

export const stopChat = (
  request: ApiRequestFn,
  conversationId: string,
  options?: { useResponses?: boolean }
): Promise<StopChatResponse> => {
  const useResponses = options?.useResponses === true;
  const path = useResponses ? '/agent_v3/chat/stop' : '/chat/stop';
  return request<StopChatResponse>(path, {
    method: 'POST',
    body: JSON.stringify({
      conversation_id: conversationId,
    }),
  });
};

export const submitRuntimeGuidance = (
  request: ApiRequestFn,
  payload: RuntimeGuidanceSubmitPayload,
): Promise<RuntimeGuidanceSubmitResponse> => {
  return request<RuntimeGuidanceSubmitResponse>('/agent_v3/chat/guide', {
    method: 'POST',
    body: JSON.stringify({
      conversation_id: payload.conversationId,
      turn_id: payload.turnId,
      content: payload.content,
      project_id: payload.projectId,
    }),
  });
};
