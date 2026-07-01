// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  AskUserPromptListResponse,
  AskUserPromptMutationPayload,
  AskUserPromptMutationResponse,
} from './types';
import type { ApiRequestFn } from './workspace';

const normalizeConversationId = (conversationId: string): string => (
  typeof conversationId === 'string' ? conversationId.trim() : ''
);

export const listAskUserPrompts = (
  request: ApiRequestFn,
  conversationId: string,
  options?: { includePending?: boolean; limit?: number },
): Promise<AskUserPromptListResponse> => {
  const normalizedConversationId = normalizeConversationId(conversationId);
  if (!normalizedConversationId) {
    throw new Error('conversationId is required');
  }
  const params = new URLSearchParams();
  params.set('conversation_id', normalizedConversationId);
  params.set('include_pending', options?.includePending === false ? 'false' : 'true');
  if (typeof options?.limit === 'number') {
    params.set('limit', String(options.limit));
  }
  return request<AskUserPromptListResponse>('/ask-user-prompts?' + params.toString());
};

export const submitAskUserPrompt = (
  request: ApiRequestFn,
  promptId: string,
  payload: AskUserPromptMutationPayload,
): Promise<AskUserPromptMutationResponse> => {
  const normalizedPromptId = String(promptId || '').trim();
  if (!normalizedPromptId) {
    throw new Error('promptId is required');
  }
  return request<AskUserPromptMutationResponse>(
    '/ask-user-prompts/' + encodeURIComponent(normalizedPromptId) + '/submit',
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
  );
};

export const cancelAskUserPrompt = (
  request: ApiRequestFn,
  promptId: string,
  payload: Pick<AskUserPromptMutationPayload, 'conversation_id' | 'conversationId' | 'reason'>,
): Promise<AskUserPromptMutationResponse> => {
  const normalizedPromptId = String(promptId || '').trim();
  if (!normalizedPromptId) {
    throw new Error('promptId is required');
  }
  return request<AskUserPromptMutationResponse>(
    '/ask-user-prompts/' + encodeURIComponent(normalizedPromptId) + '/cancel',
    {
      method: 'POST',
      body: JSON.stringify(payload),
    },
  );
};
