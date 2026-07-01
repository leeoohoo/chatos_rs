// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Message } from '../../types';

const isPersistedMessage = (message: Message): boolean => {
  const id = typeof message.id === 'string' ? message.id.trim() : '';
  return id.length > 0 && !id.startsWith('temp_');
};

const isSessionSummaryMessage = (message: Message): boolean => (
  message.role === 'assistant' && message.metadata?.type === 'session_summary'
);

export const isPendingReviewRepairMessage = (message: Message, sessionId?: string | null): boolean => {
  if (!message || !isPersistedMessage(message) || isSessionSummaryMessage(message)) {
    return false;
  }
  if (sessionId && message.sessionId !== sessionId) {
    return false;
  }
  const status = typeof message.summaryStatus === 'string'
    ? message.summaryStatus.trim().toLowerCase()
    : '';
  return status === '' || status === 'pending';
};

export const countPendingReviewRepairMessages = (
  messages: Message[],
  sessionId?: string | null,
): number => messages.reduce((count, message) => (
  count + (isPendingReviewRepairMessage(message, sessionId) ? 1 : 0)
), 0);
