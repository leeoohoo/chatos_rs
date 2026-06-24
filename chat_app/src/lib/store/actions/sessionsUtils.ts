import type { Message, Session } from '../../../types';
import { debugLog } from '@/lib/utils';
import {
  isSessionActive as isSessionActiveDomain,
  matchSessionContactProjectScope as matchSessionContactProjectScopeDomain,
  normalizeContactSessions as normalizeContactSessionsDomain,
  normalizeMemoryContact,
  PUBLIC_PROJECT_ID,
  normalizeProjectScopeId as normalizeProjectScopeIdDomain,
  resolveSessionContactIdentity as resolveSessionContactIdentityDomain,
  resolveSessionProjectScopeId as resolveSessionProjectScopeIdDomain,
  resolveSessionTimestamp as resolveSessionTimestampDomain,
  splitSessionsByMappedContacts as splitSessionsByMappedContactsDomain,
} from '../../domain/contactSessions';
import {
  normalizeDate as normalizeUnknownDate,
} from '../helpers/normalizerUtils';
import type {
  ChatState,
  SessionMessagesCacheEntry,
  SessionMessagesSnapshot,
} from '../types';

export const SESSION_MESSAGES_CACHE_MAX_ENTRIES = 16;
export const SESSION_MESSAGES_INITIAL_PAGE_SIZE = 5;

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

export const readSessionMessagesCacheFetchedAt = (
  state: Pick<ChatState, 'sessionMessagesCache'>,
  sessionId: string,
): number => {
  const cached = state.sessionMessagesCache?.[sessionId];
  return typeof cached?.fetchedAt === 'number' && Number.isFinite(cached.fetchedAt)
    ? cached.fetchedAt
    : 0;
};

export const isSessionMessagesCacheFresh = (
  state: Pick<ChatState, 'sessionMessagesCache'>,
  sessionId: string,
  options?: {
    minFetchedAt?: number;
    maxAgeMs?: number;
    now?: number;
  },
): boolean => {
  const fetchedAt = readSessionMessagesCacheFetchedAt(state, sessionId);
  if (fetchedAt <= 0) {
    return false;
  }

  const minFetchedAt = typeof options?.minFetchedAt === 'number' && Number.isFinite(options.minFetchedAt)
    ? options.minFetchedAt
    : 0;
  if (fetchedAt < minFetchedAt) {
    return false;
  }

  if (typeof options?.maxAgeMs !== 'number' || !Number.isFinite(options.maxAgeMs) || options.maxAgeMs <= 0) {
    return true;
  }

  const now = typeof options?.now === 'number' && Number.isFinite(options.now)
    ? options.now
    : Date.now();
  return now - fetchedAt <= options.maxAgeMs;
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

const isOffsetSnapshotCursor = (value: string): boolean => /^offset:\d+$/i.test(value);

const isTaskRunnerCallbackSnapshotMessage = (message: Message): boolean => {
  const messageMode = normalizeSnapshotCursor(message.messageMode);
  if (messageMode === 'task_runner_callback') {
    return true;
  }

  return normalizeSnapshotCursor(message.metadata?.task_runner_async?.message_kind) === 'task_terminal_update';
};

const countCompactHistoryUnits = (messages: Message[]): number => {
  let units = 0;
  for (let index = 0; index < messages.length; index += 1) {
    const message = messages[index];
    if (!message) {
      continue;
    }
    if (message.role === 'user') {
      units += 1;
      continue;
    }
    if (isTaskRunnerCallbackSnapshotMessage(message)) {
      continue;
    }
    const linkedUserMessageId = normalizeSnapshotCursor(message.metadata?.historyFinalForUserMessageId);
    if (!linkedUserMessageId) {
      units += 1;
    }
  }
  return units;
};

const readMessageTurnCursor = (message: Message): string => (
  normalizeSnapshotCursor(
    message?.metadata?.conversation_turn_id
    || message?.metadata?.historyProcess?.turnId
    || message?.metadata?.task_runner_async?.source_turn_id
    || message?.metadata?.historyFinalForTurnId
    || message?.id,
  )
);

export const trimCompactHistorySnapshotToRecent = (
  snapshot: SessionMessagesSnapshot | null | undefined,
  pageSize: number = SESSION_MESSAGES_INITIAL_PAGE_SIZE,
): SessionMessagesSnapshot | null => {
  if (!snapshot) {
    return null;
  }

  const requestedUnits = Number.isFinite(pageSize) ? Math.max(1, Math.floor(pageSize)) : SESSION_MESSAGES_INITIAL_PAGE_SIZE;
  const { messages } = snapshot;
  if (!Array.isArray(messages) || messages.length === 0) {
    return {
      messages: [],
      nextBefore: snapshot.nextBefore ?? null,
      loaded: snapshot.loaded,
    };
  }

  if (countCompactHistoryUnits(messages) <= requestedUnits) {
    return {
      messages: cloneStreamingMessageDraft(messages),
      nextBefore: snapshot.nextBefore ?? null,
      loaded: snapshot.loaded,
    };
  }

  let units = 0;
  let startIndex = messages.length;

  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (!message) {
      continue;
    }

    let contributesUnit = false;
    if (message.role === 'user') {
      contributesUnit = true;
    } else if (isTaskRunnerCallbackSnapshotMessage(message)) {
      contributesUnit = false;
    } else {
      const linkedUserMessageId = normalizeSnapshotCursor(message.metadata?.historyFinalForUserMessageId);
      contributesUnit = !linkedUserMessageId;
    }

    if (contributesUnit) {
      units += 1;
    }
    startIndex = index;
    if (units >= requestedUnits) {
      break;
    }
  }

  if (startIndex <= 0) {
    return {
      messages: cloneStreamingMessageDraft(messages),
      nextBefore: snapshot.nextBefore ?? null,
      loaded: snapshot.loaded,
    };
  }

  const trimmedMessages = messages.slice(startIndex);
  const normalizedSnapshotNextBefore = normalizeSnapshotCursor(snapshot.nextBefore);
  const nextBefore = isOffsetSnapshotCursor(normalizedSnapshotNextBefore)
    ? normalizedSnapshotNextBefore
    : readMessageTurnCursor(trimmedMessages[0] as Message) || normalizedSnapshotNextBefore || null;

  return {
    messages: cloneStreamingMessageDraft(trimmedMessages),
    nextBefore,
    loaded: snapshot.loaded,
  };
};

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

const preserveMissingTaskRunnerCallbacks = (
  baseMessages: Message[],
  preservedMessages: Message[],
): Message[] => {
  const baseMessageIds = new Set(baseMessages.map((message) => message.id));
  const missingCallbacks = preservedMessages.filter((message) => (
    isTaskRunnerCallbackSnapshotMessage(message)
    && !baseMessageIds.has(message.id)
  ));
  if (missingCallbacks.length === 0) {
    return baseMessages;
  }

  const baseMessageById = new Map(baseMessages.map((message) => [message.id, message]));
  const mergedMessages: Message[] = [];
  const consumedBaseIds = new Set<string>();

  for (const preservedMessage of preservedMessages) {
    const baseMessage = baseMessageById.get(preservedMessage.id);
    if (baseMessage) {
      mergedMessages.push(baseMessage);
      consumedBaseIds.add(baseMessage.id);
      continue;
    }
    if (isTaskRunnerCallbackSnapshotMessage(preservedMessage)) {
      mergedMessages.push(preservedMessage);
    }
  }

  for (const baseMessage of baseMessages) {
    if (!consumedBaseIds.has(baseMessage.id)) {
      mergedMessages.push(baseMessage);
    }
  }

  return mergedMessages;
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
      messages: preserveMissingTaskRunnerCallbacks(
        [...olderMessages, ...compactLatestMessages],
        preservedSnapshot.messages,
      ),
      nextBefore: preservedSnapshot.nextBefore,
      loaded: true,
    };
  }

  const latestIds = new Set(compactLatestMessages.map((message) => message.id));
  const overlapIndex = preservedSnapshot.messages.findIndex((message) => latestIds.has(message.id));
  if (overlapIndex < 0) {
    return {
      messages: compactLatestMessages,
      nextBefore: latestNextBefore,
      loaded: true,
    };
  }

  if (overlapIndex === 0) {
    return {
      messages: preserveMissingTaskRunnerCallbacks(
        compactLatestMessages,
        preservedSnapshot.messages,
      ),
      nextBefore: latestNextBefore,
      loaded: true,
    };
  }

  const olderMessages = preservedSnapshot.messages.slice(0, overlapIndex);
  return {
    messages: preserveMissingTaskRunnerCallbacks(
      [...olderMessages, ...compactLatestMessages],
      preservedSnapshot.messages,
    ),
    nextBefore: preservedSnapshot.nextBefore,
    loaded: true,
  };
};

type SessionProjectSyncState = Pick<ChatState, 'projects' | 'currentProjectId' | 'currentProject'>;

export const normalizeDate = normalizeUnknownDate;

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
  if (!projectId || projectId === PUBLIC_PROJECT_ID) {
    state.currentProjectId = null;
    state.currentProject = null;
    return;
  }

  state.currentProjectId = projectId;
  state.currentProject = (state.projects || []).find((project) => project.id === projectId) || null;
};

export type { MemoryContact } from '../../domain/contactSessions';

export const normalizeContact = normalizeMemoryContact;
