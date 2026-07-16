// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  StreamChatAttachmentPayload,
  StreamChatCommandResponse,
  StreamChatModelConfigPayload,
  StreamChatOptions,
} from './types';
import {
  ApiRequestError,
  buildParsedJsonErrorPayload,
} from './shared';
import { applyClientSurfaceHeader } from './surface';

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
  const headers = new Headers({
    Accept: 'application/json',
    'Content-Type': 'application/json',
  });
  if (context.accessToken) {
    headers.set('Authorization', `Bearer ${context.accessToken}`);
  }
  applyClientSurfaceHeader(headers);

  const response = await fetch(url, {
    method: 'POST',
    headers,
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
      workspace_root: options?.workspaceRoot || undefined,
      plan_mode: options?.planMode === true,
      model_config_id: modelConfig.id,
      ai_model_config: {
        temperature: modelConfig.temperature || 0.7,
        model_name: modelConfig.model_name,
        thinking_level: modelConfig.thinking_level || null,
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
