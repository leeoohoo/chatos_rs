import type { Message } from '../../../types';
import {
  isSessionActive as isSessionActiveDomain,
  matchSessionContactProjectScope as matchSessionContactProjectScopeDomain,
  normalizeContactSessions as normalizeContactSessionsDomain,
  normalizeMemoryContact,
  normalizeProjectScopeId as normalizeProjectScopeIdDomain,
  resolveSessionContactIdentity as resolveSessionContactIdentityDomain,
  resolveSessionProjectScopeId as resolveSessionProjectScopeIdDomain,
  resolveSessionTimestamp as resolveSessionTimestampDomain,
  splitSessionsByMappedContacts as splitSessionsByMappedContactsDomain,
} from '../../domain/contactSessions';
import { normalizeTurnId as normalizeMessageTurnId } from '../../domain/messages';
import {
  asRecord,
  normalizeDate as normalizeUnknownDate,
  readValue,
} from '../helpers/normalizerUtils';
import type { ChatState } from '../types';

const SESSION_MESSAGES_CACHE_MAX_ENTRIES = 16;
type SessionMessagesCacheEntry = {
  fetchedAt: number;
  messages: Message[];
};

const sessionMessagesPageCache = new Map<string, SessionMessagesCacheEntry>();

export const createPerfMeasureStopper = (measureName: string): (() => number | null) => {
  if (typeof performance === 'undefined' || typeof performance.mark !== 'function' || typeof performance.measure !== 'function') {
    return () => null;
  }

  const startMark = `${measureName}:start`;
  const endMark = `${measureName}:end`;
  performance.mark(startMark);

  return () => {
    performance.mark(endMark);
    performance.measure(measureName, startMark, endMark);
    const entries = performance.getEntriesByName(measureName);
    const duration = entries.length > 0 ? entries[entries.length - 1].duration : null;
    performance.clearMarks(startMark);
    performance.clearMarks(endMark);
    performance.clearMeasures(measureName);
    return duration;
  };
};

export const cloneStreamingMessageDraft = <T,>(value: T): T => {
  try {
    if (typeof structuredClone === 'function') {
      return structuredClone(value);
    }
  } catch {
    // ignore and fallback to JSON clone
  }

  try {
    return JSON.parse(JSON.stringify(value));
  } catch {
    return value;
  }
};

export const writeSessionMessagesCache = (sessionId: string, messages: Message[]) => {
  sessionMessagesPageCache.set(sessionId, {
    fetchedAt: Date.now(),
    messages: cloneStreamingMessageDraft(messages),
  });

  while (sessionMessagesPageCache.size > SESSION_MESSAGES_CACHE_MAX_ENTRIES) {
    const oldestKey = sessionMessagesPageCache.keys().next().value;
    if (!oldestKey) {
      break;
    }
    sessionMessagesPageCache.delete(oldestKey);
  }
};

export const deleteSessionMessagesCacheEntry = (sessionId: string) => {
  sessionMessagesPageCache.delete(sessionId);
};

type SessionTurnMapsState = Pick<ChatState, 'sessionTurnProcessState' | 'sessionTurnProcessCache'>;

export const ensureSessionTurnMaps = (state: SessionTurnMapsState, sessionId: string) => {
  if (!state.sessionTurnProcessState) {
    state.sessionTurnProcessState = {};
  }
  if (!state.sessionTurnProcessState[sessionId]) {
    state.sessionTurnProcessState[sessionId] = {};
  }

  if (!state.sessionTurnProcessCache) {
    state.sessionTurnProcessCache = {};
  }
  if (!state.sessionTurnProcessCache[sessionId]) {
    state.sessionTurnProcessCache[sessionId] = {};
  }
};

export const normalizeDate = normalizeUnknownDate;

export const normalizeTurnId = normalizeMessageTurnId;

export const resolveSessionTimestamp = resolveSessionTimestampDomain;

export const normalizeProjectScopeId = normalizeProjectScopeIdDomain;

export const resolveSessionProjectScopeId = resolveSessionProjectScopeIdDomain;

export const resolveSessionContactIdentity = resolveSessionContactIdentityDomain;

export const isSessionActive = isSessionActiveDomain;

export const matchSessionContactProjectScope = matchSessionContactProjectScopeDomain;

export const splitSessionsByMappedContacts = splitSessionsByMappedContactsDomain;

export const normalizeContactSessions = normalizeContactSessionsDomain;

export const resolveUserByTurnId = (messages: Message[], turnId: string): Message | null => {
  if (!turnId) {
    return null;
  }

  return messages.find((message) => {
    if (message?.role !== 'user') {
      return false;
    }
    const messageTurnId = normalizeTurnId(
      message?.metadata?.conversation_turn_id || message?.metadata?.historyProcess?.turnId,
    );
    return messageTurnId === turnId;
  }) || null;
};

export const buildDraftUserMessageForStreaming = (
  sessionId: string,
  draftMessage: Message | unknown,
  finalAssistantMessageId: string,
): Message | null => {
  const draftRecord = asRecord(draftMessage);
  const metadata = asRecord(readValue(draftRecord, 'metadata'));
  const draftUserRecord = asRecord(readValue(metadata, 'historyDraftUserMessage'));
  const linkedUserMessageId = normalizeTurnId(
    typeof readValue(metadata, 'historyFinalForUserMessageId') === 'string'
      ? readValue(metadata, 'historyFinalForUserMessageId')
      : (
        typeof readValue(draftUserRecord, 'id') === 'string'
          ? readValue(draftUserRecord, 'id')
          : ''
      )
  );
  const turnId = typeof readValue(metadata, 'conversation_turn_id') === 'string'
    ? readValue(metadata, 'conversation_turn_id') as string
    : '';
  const effectiveUserMessageId = linkedUserMessageId || (turnId ? `temp_user_turn_${turnId}` : '');
  if (!effectiveUserMessageId) {
    return null;
  }

  return {
    id: effectiveUserMessageId,
    sessionId,
    role: 'user' as const,
    content: typeof readValue(draftUserRecord, 'content') === 'string'
      ? readValue(draftUserRecord, 'content') as string
      : '',
    status: 'completed' as const,
    createdAt: normalizeDate(
      readValue(draftUserRecord, 'createdAt') || readValue(draftRecord, 'createdAt') || Date.now(),
    ),
    metadata: {
      ...(turnId ? { conversation_turn_id: turnId } : {}),
      historyProcess: {
        hasProcess: false,
        toolCallCount: 0,
        thinkingCount: 0,
        processMessageCount: 0,
        userMessageId: effectiveUserMessageId,
        turnId,
        finalAssistantMessageId: finalAssistantMessageId || null,
        expanded: false,
        loaded: false,
        loading: false,
      },
    },
  };
};

export type { MemoryContact } from '../../domain/contactSessions';

export const normalizeContact = normalizeMemoryContact;
