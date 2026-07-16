// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  RuntimeGuidanceCommandResponse,
  StopChatResponse,
  StreamChatAttachmentPayload,
  StreamChatCommandResponse,
  StreamChatModelConfigPayload,
  StreamChatOptions,
} from '../client/types';
import { requestLocalRuntime } from './bridge';
import { LocalRuntimeSessionClient } from './sessionClient';

export class LocalRuntimeClient extends LocalRuntimeSessionClient {
  async sendChatCommand(
    conversationId: string,
    content: string,
    modelConfig: StreamChatModelConfigPayload,
    attachments?: StreamChatAttachmentPayload[],
    reasoningEnabled?: boolean,
    options?: StreamChatOptions,
  ): Promise<StreamChatCommandResponse> {
    return requestLocalRuntime<StreamChatCommandResponse>(
      '/api/local/runtime/chat/send',
      {
        method: 'POST',
        body: JSON.stringify({
          conversation_id: conversationId,
          content,
          attachments: attachments || [],
          reasoning_enabled: reasoningEnabled,
          turn_id: options?.turnId,
          idempotency_key: options?.turnId,
          model_config_id: modelConfig.id,
          system_prompt: options?.systemPrompt || undefined,
          ai_model_config: {
            temperature: modelConfig.temperature,
            model_name: modelConfig.model_name,
            thinking_level: modelConfig.thinking_level || null,
          },
        }),
      },
    );
  }

  async sendRuntimeGuidance(
    conversationId: string,
    turnId: string,
    content: string,
    attachments?: StreamChatAttachmentPayload[],
  ): Promise<RuntimeGuidanceCommandResponse> {
    return requestLocalRuntime<RuntimeGuidanceCommandResponse>(
      '/api/local/runtime/chat/guidance',
      {
        method: 'POST',
        body: JSON.stringify({
          conversation_id: conversationId,
          turn_id: turnId,
          content,
          attachments: attachments || [],
        }),
      },
    );
  }

  async stopChat(conversationId: string, turnId?: string | null): Promise<StopChatResponse> {
    return requestLocalRuntime<StopChatResponse>(
      '/api/local/runtime/chat/stop',
      {
        method: 'POST',
        body: JSON.stringify({
          conversation_id: conversationId,
          turn_id: turnId || undefined,
        }),
      },
    );
  }
}
