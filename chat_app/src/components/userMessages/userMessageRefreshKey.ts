// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Message } from '../../types';

export const getLatestUserMessageRefreshKey = (
  messages: Message[],
  sessionId: string | null | undefined,
): string | null => {
  const normalizedSessionId = typeof sessionId === 'string' ? sessionId.trim() : '';
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (!message || message.role !== 'user') {
      continue;
    }
    if (normalizedSessionId && message.sessionId !== normalizedSessionId) {
      continue;
    }
    const messageId = typeof message.id === 'string' ? message.id.trim() : '';
    if (messageId) {
      return messageId;
    }
  }
  return null;
};
