import type { Message, Session } from '../../../types';
import { debugLog } from '@/lib/utils';
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
import type {
  ChatState,
  SessionMessagesCacheEntry,
  SessionMessagesSnapshot,
} from '../types';

export const SESSION_MESSAGES_CACHE_MAX_ENTRIES = 16;

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

type SessionMessagesCacheState = Pick<ChatState, 'sessionMessagesCache' | 'sessionMessagesCacheOrder'>;
type CurrentSessionViewState = Pick<
  ChatState,
  'currentSessionId' | 'currentSession' | 'messages' | 'selectedModelId' | 'selectedAgentId' | 'isLoading' | 'isStreaming' | 'streamingMessageId' | 'hasMoreMessages'
>;

const ensureSessionMessagesCacheState = (state: SessionMessagesCacheState) => {
  if (!state.sessionMessagesCache) {
    state.sessionMessagesCache = {};
  }
  if (!Array.isArray(state.sessionMessagesCacheOrder)) {
    state.sessionMessagesCacheOrder = [];
  }
};

const buildSessionMessagesCacheLogPayload = (
  state: SessionMessagesCacheState,
  sessionId: string,
  snapshot?: SessionMessagesSnapshot | SessionMessagesCacheEntry | null,
) => ({
  sessionId,
  messageCount: Array.isArray(snapshot?.messages) ? snapshot.messages.length : 0,
  nextBefore: snapshot?.nextBefore ?? null,
  loaded: snapshot?.loaded === true,
  cacheSize: state.sessionMessagesCacheOrder.length,
  cacheOrder: [...state.sessionMessagesCacheOrder],
});

export const writeSessionMessagesCache = (
  state: SessionMessagesCacheState,
  sessionId: string,
  payload: SessionMessagesSnapshot,
) => {
  ensureSessionMessagesCacheState(state);
  const nextEntry: SessionMessagesCacheEntry = {
    fetchedAt: Date.now(),
    messages: cloneStreamingMessageDraft(payload.messages),
    nextBefore: payload.nextBefore,
    loaded: payload.loaded,
  };
  state.sessionMessagesCache[sessionId] = nextEntry;
  state.sessionMessagesCacheOrder = [
    sessionId,
    ...state.sessionMessagesCacheOrder.filter((item) => item !== sessionId),
  ];

  const evictedSessionIds: string[] = [];
  while (state.sessionMessagesCacheOrder.length > SESSION_MESSAGES_CACHE_MAX_ENTRIES) {
    const oldestKey = state.sessionMessagesCacheOrder.pop();
    if (!oldestKey) {
      break;
    }
    delete state.sessionMessagesCache[oldestKey];
    evictedSessionIds.push(oldestKey);
  }

  debugLog('[Store] sessionMessagesCache write', {
    ...buildSessionMessagesCacheLogPayload(state, sessionId, nextEntry),
    evicted: evictedSessionIds.length > 0,
    evictedSessionIds,
  });
};

export const readSessionMessagesCache = (
  state: Pick<ChatState, 'sessionMessagesCache'>,
  sessionId: string,
): SessionMessagesSnapshot | null => {
  const cached = state.sessionMessagesCache?.[sessionId];
  if (!cached) {
    return null;
  }
  return {
    messages: cloneStreamingMessageDraft(cached.messages),
    nextBefore: cached.nextBefore,
    loaded: cached.loaded,
  };
};

export const touchSessionMessagesCacheEntry = (
  state: SessionMessagesCacheState,
  sessionId: string,
): boolean => {
  ensureSessionMessagesCacheState(state);
  if (!state.sessionMessagesCache[sessionId]) {
    return false;
  }

  state.sessionMessagesCacheOrder = [
    sessionId,
    ...state.sessionMessagesCacheOrder.filter((item) => item !== sessionId),
  ];
  debugLog('[Store] sessionMessagesCache touch', {
    ...buildSessionMessagesCacheLogPayload(
      state,
      sessionId,
      state.sessionMessagesCache[sessionId],
    ),
  });
  return true;
};

export const clearSessionMessagesCache = (state: SessionMessagesCacheState) => {
  const clearedSessionIds = Array.isArray(state.sessionMessagesCacheOrder)
    ? [...state.sessionMessagesCacheOrder]
    : Object.keys(state.sessionMessagesCache || {});
  state.sessionMessagesCache = {};
  state.sessionMessagesCacheOrder = [];
  debugLog('[Store] sessionMessagesCache clear', {
    clearedCount: clearedSessionIds.length,
    clearedSessionIds,
  });
};

export const deleteSessionMessagesCacheEntry = (
  state: SessionMessagesCacheState,
  sessionId: string,
) => {
  ensureSessionMessagesCacheState(state);
  const deletedEntry = state.sessionMessagesCache[sessionId];
  delete state.sessionMessagesCache[sessionId];
  state.sessionMessagesCacheOrder = state.sessionMessagesCacheOrder.filter((item) => item !== sessionId);
  debugLog('[Store] sessionMessagesCache delete', {
    ...buildSessionMessagesCacheLogPayload(state, sessionId, deletedEntry),
    existed: Boolean(deletedEntry),
  });
};

export const resetCurrentSessionViewState = (state: CurrentSessionViewState) => {
  state.currentSessionId = null;
  state.currentSession = null;
  state.selectedModelId = null;
  state.selectedAgentId = null;
  state.messages = [];
  state.isLoading = false;
  state.isStreaming = false;
  state.streamingMessageId = null;
  state.hasMoreMessages = false;
};

const normalizeSnapshotCursor = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

const readMessageTurnCursor = (message: Message): string => (
  normalizeSnapshotCursor(
    message?.metadata?.conversation_turn_id
    || message?.metadata?.historyProcess?.turnId
    || message?.metadata?.historyFinalForTurnId
    || message?.id,
  )
);

const matchesSnapshotCursor = (message: Message, cursor: string): boolean => {
  if (message?.role !== 'user') {
    return false;
  }

  const normalizedCursor = normalizeSnapshotCursor(cursor);
  if (!normalizedCursor) {
    return false;
  }

  return message.id === normalizedCursor || readMessageTurnCursor(message) === normalizedCursor;
};

export const extractCompactHistoryMessages = (messages: Message[]): Message[] => messages.filter((message) => {
  if (!message || message.role === 'tool') {
    return false;
  }

  if (message.status === 'streaming' && message.role === 'assistant') {
    return false;
  }

  if (message.metadata?.historyProcessPlaceholder === true) {
    return false;
  }

  const processUserMessageId = normalizeSnapshotCursor(message.metadata?.historyProcessUserMessageId);
  const processTurnId = normalizeSnapshotCursor(message.metadata?.historyProcessTurnId);
  return !processUserMessageId && !processTurnId;
});

export const readVisibleSessionMessagesSnapshot = (
  state: Pick<ChatState, 'currentSessionId' | 'messages' | 'sessionMessagePaginationState'>,
  sessionId: string,
): SessionMessagesSnapshot | null => {
  if (state.currentSessionId !== sessionId) {
    return null;
  }

  const pagination = state.sessionMessagePaginationState?.[sessionId];
  const compactMessages = extractCompactHistoryMessages(
    (state.messages || []).filter((message) => message?.sessionId === sessionId),
  );
  if (compactMessages.length === 0 && pagination?.loaded !== true) {
    return null;
  }

  return {
    messages: cloneStreamingMessageDraft(compactMessages),
    nextBefore: pagination?.nextBefore ?? null,
    loaded: pagination?.loaded === true,
  };
};

export const mergeLatestCompactHistorySnapshot = (
  latestMessages: Message[],
  latestNextBefore: string | null,
  preservedSnapshot: SessionMessagesSnapshot | null | undefined,
): SessionMessagesSnapshot => {
  const compactLatestMessages = extractCompactHistoryMessages(latestMessages);
  const normalizedCursor = normalizeSnapshotCursor(latestNextBefore);

  if (!preservedSnapshot?.loaded) {
    return {
      messages: compactLatestMessages,
      nextBefore: latestNextBefore,
      loaded: true,
    };
  }

  const splitIndex = normalizedCursor
    ? preservedSnapshot.messages.findIndex((message) => matchesSnapshotCursor(message, normalizedCursor))
    : -1;
  if (splitIndex > 0) {
    const olderMessages = preservedSnapshot.messages.slice(0, splitIndex);
    return {
      messages: [...olderMessages, ...compactLatestMessages],
      nextBefore: preservedSnapshot.nextBefore,
      loaded: true,
    };
  }

  const latestIds = new Set(compactLatestMessages.map((message) => message.id));
  const overlapIndex = preservedSnapshot.messages.findIndex((message) => latestIds.has(message.id));
  if (overlapIndex <= 0) {
    return {
      messages: compactLatestMessages,
      nextBefore: latestNextBefore,
      loaded: true,
    };
  }

  const olderMessages = preservedSnapshot.messages.slice(0, overlapIndex);
  return {
    messages: [...olderMessages, ...compactLatestMessages],
    nextBefore: preservedSnapshot.nextBefore,
    loaded: true,
  };
};

type SessionTurnMapsState = Pick<ChatState, 'sessionTurnProcessState' | 'sessionTurnProcessCache'>;
type SessionProjectSyncState = Pick<ChatState, 'projects' | 'currentProjectId' | 'currentProject'>;

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

export const syncCurrentProjectFromSession = (
  state: SessionProjectSyncState,
  session: Session | null | undefined,
) => {
  const projectId = resolveSessionProjectScopeId(session);
  if (!projectId || projectId === '0') {
    state.currentProjectId = null;
    state.currentProject = null;
    return;
  }

  state.currentProjectId = projectId;
  state.currentProject = (state.projects || []).find((project) => project.id === projectId) || null;
};

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
