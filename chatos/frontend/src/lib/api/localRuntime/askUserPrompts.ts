// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  AskUserPromptListResponse,
  AskUserPromptMutationPayload,
  AskUserPromptMutationResponse,
} from '../client/types';
import { requestLocalRuntime } from './bridge';

export const listLocalAskUserPrompts = (
  sessionId: string,
  options: { includePending?: boolean; limit?: number } = {},
): Promise<AskUserPromptListResponse> => {
  const query = new URLSearchParams();
  query.set('include_pending', options.includePending === false ? 'false' : 'true');
  if (Number.isFinite(options.limit)) {
    query.set('limit', String(Math.max(1, Math.trunc(options.limit || 100))));
  }
  return requestLocalRuntime<AskUserPromptListResponse>(
    `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}/ask-user-prompts?${query}`,
  );
};

export const submitLocalAskUserPrompt = (
  sessionId: string,
  promptId: string,
  payload: AskUserPromptMutationPayload,
): Promise<AskUserPromptMutationResponse> => requestLocalRuntime<AskUserPromptMutationResponse>(
  promptMutationPath(sessionId, promptId, 'submit'),
  { method: 'POST', body: JSON.stringify(payload) },
);

export const cancelLocalAskUserPrompt = (
  sessionId: string,
  promptId: string,
  payload: Pick<AskUserPromptMutationPayload, 'conversation_id' | 'conversationId' | 'reason'>,
): Promise<AskUserPromptMutationResponse> => requestLocalRuntime<AskUserPromptMutationResponse>(
  promptMutationPath(sessionId, promptId, 'cancel'),
  { method: 'POST', body: JSON.stringify(payload) },
);

export const askUserSessionId = (payload: AskUserPromptMutationPayload): string => {
  const sessionId = String(payload.conversation_id || payload.conversationId || '').trim();
  if (!sessionId.startsWith('lc_session_')) {
    throw new Error('Local Ask User requires a local conversation ID');
  }
  return sessionId;
};

const promptMutationPath = (
  sessionId: string,
  promptId: string,
  action: 'submit' | 'cancel',
): string => (
  `/api/local/runtime/sessions/${encodeURIComponent(sessionId)}`
  + `/ask-user-prompts/${encodeURIComponent(promptId)}/${action}`
);
