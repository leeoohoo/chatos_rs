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
  const url = `${context.baseUrl}/agent/chat/send`;
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
      auto_create_task: options?.autoCreateTask,
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
  turnId?: string | null,
): Promise<StopChatResponse> => {
  return request<StopChatResponse>('/agent/chat/stop', {
    method: 'POST',
    body: JSON.stringify({
      conversation_id: conversationId,
      ...(typeof turnId === 'string' && turnId.trim() ? { turn_id: turnId.trim() } : {}),
    }),
  });
};

export const submitRuntimeGuidance = (
  request: ApiRequestFn,
  payload: RuntimeGuidanceSubmitPayload,
): Promise<RuntimeGuidanceSubmitResponse> => {
  return request<RuntimeGuidanceSubmitResponse>('/agent/chat/guide', {
    method: 'POST',
    body: JSON.stringify({
      conversation_id: payload.conversationId,
      turn_id: payload.turnId,
      content: payload.content,
      project_id: payload.projectId,
    }),
  });
};
