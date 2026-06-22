import { normalizeRawMessages } from '../../../domain/messages';
import type { SessionMessageResponse } from '../../../api/client/types';
import type { StreamingMessage } from './types';

export const normalizePersistedMessage = (
  rawMessage: unknown,
  sessionId: string,
): StreamingMessage | null => {
  if (!rawMessage || typeof rawMessage !== 'object' || Array.isArray(rawMessage)) {
    return null;
  }

  const normalized = normalizeRawMessages([rawMessage as SessionMessageResponse], sessionId);
  return normalized.length > 0 ? normalized[0] as StreamingMessage : null;
};
