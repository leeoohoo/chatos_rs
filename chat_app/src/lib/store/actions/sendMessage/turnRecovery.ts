import type ApiClient from '../../../api/client';
import type {
  SessionMessageResponse,
  TurnRuntimeSnapshotLookupResponse,
} from '../../../api/client/types';
import { normalizeRawMessages } from '../../../domain/messages';
import {
  ensureSessionTurnMaps,
  writeTurnProcessCache,
} from '../messagesState';
import type {
  ChatStoreDraft,
  ChatStoreSet,
} from '../../types';
import {
  createDefaultHistoryProcessState,
  type StreamingMessage,
} from './types';
import {
  canUseLocalTerminalAssistant,
  findLocalTurnAssistantCandidate,
} from './persistedTurnMessages';
import {
  createDefaultSessionChatState,
} from './sessionState';

type RuntimeContextApiClient = Pick<
  ApiClient,
  | 'getConversationLatestTurnRuntimeContext'
  | 'getConversationTurnRuntimeContextByTurn'
  | 'getConversationTurnMessagesByTurn'
  | 'getConversationTurnMessages'
>;

type RecoverStreamingTurnBySnapshotParams = {
  apiClient: RuntimeContextApiClient;
  set: ChatStoreSet;
  sessionId: string;
  turnId: string;
  tempAssistantMessageId: string;
  tempUserId: string | null;
  preferredUserMessageId?: string | null;
};

const TERMINAL_SNAPSHOT_STATUSES = new Set(['completed', 'failed', 'error', 'cancelled', 'canceled']);

const normalizeSnapshotStatus = (value: unknown): string => (
  typeof value === 'string' ? value.trim().toLowerCase() : ''
);

const isUserMessage = (message: StreamingMessage): boolean => message.role === 'user';
const isAssistantMessage = (message: StreamingMessage): boolean => message.role === 'assistant';

const readLinkedUserId = (message: StreamingMessage | null | undefined): string => {
  const value = message?.metadata?.historyFinalForUserMessageId;
  return typeof value === 'string' ? value.trim() : '';
};

const readDraftUserId = (message: StreamingMessage | null | undefined): string => {
  const value = message?.metadata?.historyDraftUserMessage?.id;
  return typeof value === 'string' ? value.trim() : '';
};

const settleLocalTerminalAssistant = (
  state: ChatStoreDraft,
  {
    sessionId,
    turnId,
    tempAssistantMessageId,
    tempUserId,
    preferredUserMessageId,
    snapshotStatus,
  }: {
    sessionId: string;
    turnId: string;
    tempAssistantMessageId: string;
    tempUserId: string | null;
    preferredUserMessageId?: string | null;
    snapshotStatus: string;
  },
): boolean => {
  const localAssistant = findLocalTurnAssistantCandidate(
    state.messages as StreamingMessage[],
    tempAssistantMessageId,
    tempUserId,
    turnId,
  );
  if (
    !localAssistant
    || !canUseLocalTerminalAssistant(localAssistant, {
      expectedTurnId: turnId,
      tempUserId,
      requireTerminalStatus: false,
    })
  ) {
    return false;
  }

  const linkedUserId = readLinkedUserId(localAssistant)
    || preferredUserMessageId?.trim()
    || readDraftUserId(localAssistant)
    || tempUserId?.trim()
    || '';
  const assistantIndex = state.messages.findIndex((message) => message.id === localAssistant.id);
  if (assistantIndex < 0) {
    return false;
  }

  state.messages[assistantIndex] = {
    ...state.messages[assistantIndex],
    status: snapshotStatus === 'completed' ? 'completed' : 'error',
    metadata: {
      ...(state.messages[assistantIndex].metadata || {}),
      ...(linkedUserId ? { historyFinalForUserMessageId: linkedUserId } : {}),
      ...(turnId ? { historyFinalForTurnId: turnId } : {}),
    },
  };

  if (tempAssistantMessageId && localAssistant.id !== tempAssistantMessageId) {
    state.messages = state.messages.filter((message) => message.id !== tempAssistantMessageId);
  }

  if (linkedUserId) {
    const userIndex = state.messages.findIndex((message) => (
      message.role === 'user' && message.id === linkedUserId
    ));
    if (userIndex >= 0) {
      const existingUser = state.messages[userIndex];
      const existingMeta = existingUser?.metadata || {};
      const existingHistoryProcess = existingMeta.historyProcess;
      if (existingHistoryProcess && typeof existingHistoryProcess === 'object') {
        state.messages[userIndex] = {
          ...existingUser,
          metadata: {
            ...existingMeta,
            historyProcess: {
              ...existingHistoryProcess,
              userMessageId: existingUser.id,
              turnId,
              finalAssistantMessageId: localAssistant.id,
            },
          },
        };
      }
    }
  }

  settleRecoveredStreamingState(state, sessionId, localAssistant.id, snapshotStatus);
  return true;
};

const upsertRecoveredTurnMessages = (
  state: ChatStoreDraft,
  sessionId: string,
  turnId: string,
  recoveredMessages: StreamingMessage[],
) => {
  if (recoveredMessages.length === 0) {
    return false;
  }

  ensureSessionTurnMaps(state, sessionId);

  const existingById = new Map(state.messages.map((message, index) => [message.id, index] as const));
  const recoveredUser = recoveredMessages.find((message) => isUserMessage(message)) || null;
  const recoveredAssistant = recoveredMessages.find((message) => (
    isAssistantMessage(message) && !message.metadata?.historyProcessUserMessageId
  )) || null;
  const processMessages = recoveredMessages.filter((message) => (
    message.metadata?.historyProcessUserMessageId || message.metadata?.historyProcessTurnId
  ));
  const processKey = turnId;

  if (recoveredUser) {
    const nextHistoryProcess = {
      ...createDefaultHistoryProcessState({
        userMessageId: recoveredUser.id,
        turnId,
        finalAssistantMessageId: recoveredAssistant?.id || null,
      }),
      ...(recoveredUser.metadata?.historyProcess || {}),
      userMessageId: recoveredUser.id,
      turnId,
      finalAssistantMessageId: recoveredAssistant?.id || recoveredUser.metadata?.historyProcess?.finalAssistantMessageId || null,
      loaded: true,
      loading: false,
    };
    recoveredUser.metadata = {
      ...(recoveredUser.metadata || {}),
      historyProcess: nextHistoryProcess,
    };
    writeTurnProcessCache(
      state.sessionTurnProcessCache[sessionId],
      processKey,
      recoveredUser.id,
      processMessages,
    );
  }

  recoveredMessages.forEach((message) => {
    const existingIndex = existingById.get(message.id);
    if (typeof existingIndex === 'number') {
      state.messages[existingIndex] = {
        ...state.messages[existingIndex],
        ...message,
      };
      return;
    }

    const insertBeforeFinalAssistantIndex = recoveredAssistant && message.id !== recoveredAssistant.id
      ? state.messages.findIndex((item) => item.id === recoveredAssistant.id)
      : -1;
    if (insertBeforeFinalAssistantIndex >= 0) {
      state.messages.splice(insertBeforeFinalAssistantIndex, 0, message);
      return;
    }
    state.messages.push(message);
  });

  return true;
};

const settleRecoveredStreamingState = (
  state: ChatStoreDraft,
  sessionId: string,
  assistantMessageId: string,
  snapshotStatus: string,
) => {
  const terminalStatus = normalizeSnapshotStatus(snapshotStatus);
  if (terminalStatus === 'running') {
    const prev = state.sessionChatState?.[sessionId] || createDefaultSessionChatState();
    state.sessionChatState[sessionId] = {
      ...prev,
      isLoading: true,
      isStreaming: true,
      isStopping: false,
      streamingMessageId: assistantMessageId,
      activeTurnId: turnIdFromState(state, sessionId) || prev.activeTurnId || null,
      streamingTransport: 'realtime',
    };
    if (state.currentSessionId === sessionId) {
      state.isLoading = true;
      state.isStreaming = true;
      state.streamingMessageId = assistantMessageId;
    }
    return;
  }

  const assistantIndex = state.messages.findIndex((message) => message.id === assistantMessageId);
  if (assistantIndex >= 0) {
    state.messages[assistantIndex] = {
      ...state.messages[assistantIndex],
      status: terminalStatus === 'completed' ? 'completed' : 'error',
    };
  }

  if (state.sessionStreamingMessageDrafts) {
    state.sessionStreamingMessageDrafts[sessionId] = null;
  }

  const prev = state.sessionChatState?.[sessionId] || createDefaultSessionChatState();
  state.sessionChatState[sessionId] = {
    ...prev,
    isLoading: false,
    isStreaming: false,
    isStopping: false,
    streamingMessageId: null,
    activeTurnId: null,
    streamingPreviewText: '',
    streamingTransport: null,
  };
  if (state.currentSessionId === sessionId) {
    state.isLoading = false;
    state.isStreaming = false;
    state.streamingMessageId = null;
  }
};

const turnIdFromState = (state: ChatStoreDraft, sessionId: string): string | null => {
  const value = state.sessionChatState?.[sessionId]?.activeTurnId;
  return typeof value === 'string' && value.trim().length > 0 ? value.trim() : null;
};

export const recoverStreamingTurnBySnapshot = async ({
  apiClient,
  set,
  sessionId,
  turnId,
  tempAssistantMessageId,
  tempUserId,
  preferredUserMessageId,
}: RecoverStreamingTurnBySnapshotParams): Promise<{
  snapshot: TurnRuntimeSnapshotLookupResponse | null;
  recovered: boolean;
  terminal: boolean;
}> => {
  let snapshot: TurnRuntimeSnapshotLookupResponse | null = null;
  try {
    snapshot = await apiClient.getConversationTurnRuntimeContextByTurn(sessionId, turnId);
  } catch (error) {
    console.error('Failed to load turn runtime snapshot during turn recovery:', error);
  }
  if (
    (!snapshot || snapshot.snapshot_source === 'missing')
    && sessionId
  ) {
    try {
      const latestSnapshot = await apiClient.getConversationLatestTurnRuntimeContext(sessionId);
      const latestTurnId = typeof latestSnapshot?.turn_id === 'string'
        ? latestSnapshot.turn_id.trim()
        : '';
      if (latestTurnId === turnId) {
        snapshot = latestSnapshot;
      }
    } catch (error) {
      console.error('Failed to load latest runtime snapshot during turn recovery:', error);
    }
  }
  const snapshotStatus = normalizeSnapshotStatus(snapshot?.status);
  const shouldPullTurnMessages = (
    !snapshot
    || snapshot.snapshot_source === 'missing'
    || TERMINAL_SNAPSHOT_STATUSES.has(snapshotStatus)
    || snapshotStatus === 'running'
  );
  if (!shouldPullTurnMessages) {
    return {
      snapshot,
      recovered: false,
      terminal: false,
    };
  }

  let rawMessages: SessionMessageResponse[] = [];
  try {
    rawMessages = await apiClient.getConversationTurnMessagesByTurn(sessionId, turnId);
  } catch (error) {
    console.error('Failed to load turn messages by turn during turn recovery:', error);
  }
  if (rawMessages.length === 0 && preferredUserMessageId) {
    try {
      rawMessages = await apiClient.getConversationTurnMessages(sessionId, preferredUserMessageId);
    } catch (error) {
      console.error('Failed to load turn messages by user message during turn recovery:', error);
    }
  }
  const recoveredMessages = normalizeRawMessages(rawMessages, sessionId) as StreamingMessage[];
  if (recoveredMessages.length === 0) {
    let recoveredLocally = false;
    if (TERMINAL_SNAPSHOT_STATUSES.has(snapshotStatus)) {
      set((state) => {
        recoveredLocally = settleLocalTerminalAssistant(state, {
          sessionId,
          turnId,
          tempAssistantMessageId,
          tempUserId,
          preferredUserMessageId,
          snapshotStatus,
        });
      });
    }
    return {
      snapshot,
      recovered: recoveredLocally,
      terminal: TERMINAL_SNAPSHOT_STATUSES.has(snapshotStatus),
    };
  }

  set((state) => {
    const recoveredUser = recoveredMessages.find((message) => isUserMessage(message)) || null;
    const assistantForSettle = recoveredMessages.find((message) => (
      isAssistantMessage(message) && !message.metadata?.historyProcessUserMessageId
    ));
    const resolvedAssistantId = assistantForSettle?.id || tempAssistantMessageId;
    const applied = upsertRecoveredTurnMessages(state, sessionId, turnId, recoveredMessages);
    if (!applied) {
      return;
    }

    if (tempUserId && recoveredUser && tempUserId !== recoveredUser.id) {
      state.messages = state.messages.filter((message) => message.id !== tempUserId);
    }
    if (tempAssistantMessageId && resolvedAssistantId !== tempAssistantMessageId) {
      state.messages = state.messages.filter((message) => message.id !== tempAssistantMessageId);
    } else if (!assistantForSettle) {
      const tempAssistantIndex = state.messages.findIndex((message) => message.id === tempAssistantMessageId);
      if (tempAssistantIndex >= 0) {
        const existingAssistant = state.messages[tempAssistantIndex];
        const linkedUserId = recoveredUser?.id
          || readLinkedUserId(existingAssistant)
          || preferredUserMessageId
          || readDraftUserId(existingAssistant);
        state.messages[tempAssistantIndex] = {
          ...existingAssistant,
          status: snapshotStatus === 'completed' ? 'completed' : 'error',
          metadata: {
            ...(existingAssistant.metadata || {}),
            ...(linkedUserId ? { historyFinalForUserMessageId: linkedUserId } : {}),
            ...(turnId ? { historyFinalForTurnId: turnId } : {}),
          },
        };
      }
    }

    settleRecoveredStreamingState(state, sessionId, resolvedAssistantId, snapshotStatus);
  });

  return {
    snapshot,
    recovered: true,
    terminal: TERMINAL_SNAPSHOT_STATUSES.has(snapshotStatus),
  };
};
