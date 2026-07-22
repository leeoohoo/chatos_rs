// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useConversationChatStreamRealtime } from '../../lib/realtime/useConversationChatStreamRealtime';
import type { Message } from '../../types';

export const readTurnId = (message: Message | null | undefined): string => {
  const metadata = message?.metadata || {};
  const taskRunnerAsync = metadata.task_runner_async;
  const taskRunnerRecord = taskRunnerAsync && typeof taskRunnerAsync === 'object'
    ? taskRunnerAsync as Record<string, unknown>
    : {};
  const value = (
    metadata.conversation_turn_id
    || metadata.conversationTurnId
    || taskRunnerRecord.source_turn_id
    || taskRunnerRecord.sourceTurnId
  );
  return typeof value === 'string' ? value.trim() : '';
};

export const readString = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

export const normalizeEventType = (
  payload: Parameters<Parameters<typeof useConversationChatStreamRealtime>[0]['onEvent']>[0],
  eventName?: string,
): string => String(payload.raw?.type || payload.stream_type || eventName || '').trim().toLowerCase();

export const isCancelledEventType = (eventType: string): boolean => (
  eventType === 'cancelled'
  || eventType === 'canceled'
  || eventType.endsWith('.cancelled')
  || eventType.endsWith('.canceled')
);

export const isFailedEventType = (eventType: string): boolean => (
  eventType === 'error'
  || eventType === 'failed'
  || eventType.endsWith('.failed')
  || eventType.endsWith('.error')
);

export const isTerminalErrorEventType = (eventType: string): boolean => (
  isFailedEventType(eventType) || isCancelledEventType(eventType)
);

export const readRealtimeErrorMessage = (
  payload: Parameters<Parameters<typeof useConversationChatStreamRealtime>[0]['onEvent']>[0],
): string | null => {
  const direct = readString(payload.raw?.message);
  if (direct) {
    return direct;
  }
  const data = payload.raw?.data;
  if (data && typeof data === 'object' && !Array.isArray(data)) {
    const record = data as Record<string, unknown>;
    return readString(record.message) || readString(record.error) || null;
  }
  return null;
};

export const sanitizeMessageIdPart = (value: string): string => (
  value.replace(/[^A-Za-z0-9_-]/g, '_')
);
