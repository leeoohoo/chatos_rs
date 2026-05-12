import type {
  RuntimeGuidanceSubmitPayload,
  RuntimeGuidanceSubmitResponse,
  StopChatResponse,
  StreamChatAttachmentPayload,
  StreamChatCommandResponse,
  StreamChatModelConfigPayload,
  StreamChatOptions,
} from './types';
import {
  ApiRequestError,
  buildParsedJsonErrorPayload,
} from './shared';
import type { ApiRequestFn } from './workspace';

export interface StreamApiContext {
  baseUrl: string;
  accessToken: string | null;
  applyRefreshedAccessToken: (response: Response) => void;
}

const buildStreamHttpError = async (response: Response): Promise<ApiRequestError> => {
  const status = response.status;
  const raw = await response.text().catch(() => '');
  const {
    message,
    code,
    payload,
  } = buildParsedJsonErrorPayload(raw, '请求失败');
  const normalizedMessage = message.trim().length > 0 ? message.trim() : '请求失败';
  return new ApiRequestError(`HTTP ${status}: ${normalizedMessage}`, {
    status,
    code,
    payload,
  });
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
      skills_enabled: options?.skillsEnabled === true,
      selected_skill_ids: options?.selectedSkillIds || [],
      model_config_id: modelConfig.id,
      ai_model_config: {
        temperature: modelConfig.temperature || 0.7,
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

export const sendChatCommand = async (
  context: StreamApiContext,
  conversationId: string,
  content: string,
  modelConfig: StreamChatModelConfigPayload,
  userId?: string,
  attachments?: StreamChatAttachmentPayload[],
  reasoningEnabled?: boolean,
  options?: StreamChatOptions,
): Promise<StreamChatCommandResponse> => {
  const useResponses = modelConfig?.supports_responses === true;
  const url = `${context.baseUrl}/${useResponses ? 'agent_v3' : 'agent_v2'}/chat/send`;
  const hasRemoteConnectionId = Boolean(
    options && Object.prototype.hasOwnProperty.call(options, 'remoteConnectionId'),
  );

  const response = await fetch(url, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...(context.accessToken ? { Authorization: `Bearer ${context.accessToken}` } : {}),
      Accept: 'application/json',
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
      skills_enabled: options?.skillsEnabled === true,
      selected_skill_ids: options?.selectedSkillIds || [],
      model_config_id: modelConfig.id,
      ai_model_config: {
        temperature: modelConfig.temperature || 0.7,
      },
    }),
  });
  context.applyRefreshedAccessToken(response);

  if (!response.ok) {
    throw await buildStreamHttpError(response);
  }

  const raw = await response.text().catch(() => '');
  if (!raw) {
    return {
      accepted: true,
      conversation_id: conversationId,
      turn_id: options?.turnId || null,
    };
  }

  try {
    return JSON.parse(raw) as StreamChatCommandResponse;
  } catch {
    return {
      accepted: true,
      conversation_id: conversationId,
      turn_id: options?.turnId || null,
    };
  }
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
