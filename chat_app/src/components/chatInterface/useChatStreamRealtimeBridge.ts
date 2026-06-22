import { useMemo } from 'react';
import { shallow } from 'zustand/shallow';

import {
  useRealtimeConnectionState,
  useRealtimeTopics,
} from '../../lib/realtime/RealtimeProvider';
import { useConversationChatStreamRealtime } from '../../lib/realtime/useConversationChatStreamRealtime';
import { normalizePersistedMessage } from '../../lib/store/actions/sendMessage/persistedTurnMessages';
import { createDefaultSessionChatState } from '../../lib/store/actions/sendMessage/sessionState';
import {
  useChatStoreContext,
  useChatStoreSelector,
} from '../../lib/store/ChatStoreContext';
import type { Message } from '../../types';
import type {
  ChatStoreDraft,
  ChatStoreSet,
} from '../../lib/store/types';

const readTurnId = (message: Message | null | undefined): string => {
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

const mergeRealtimeMessage = (
  existing: Message | undefined,
  incoming: Message,
): Message => {
  const existingMetadata = existing?.metadata || {};
  const incomingMetadata = incoming.metadata || {};
  return {
    ...(existing || {}),
    ...incoming,
    metadata: {
      ...existingMetadata,
      ...incomingMetadata,
      historyProcess: existingMetadata.historyProcess || incomingMetadata.historyProcess,
    },
  };
};

const upsertRealtimeMessage = (
  messages: Message[],
  incoming: Message | null,
): Message[] => {
  if (!incoming) {
    return messages;
  }

  const nextMessages = [...messages];
  const existingIndex = nextMessages.findIndex((message) => message.id === incoming.id);
  if (existingIndex >= 0) {
    nextMessages[existingIndex] = mergeRealtimeMessage(nextMessages[existingIndex], incoming);
    return nextMessages;
  }

  const incomingTurnId = readTurnId(incoming);
  const optimisticIndex = incomingTurnId
    ? nextMessages.findIndex((message) => (
      message.role === incoming.role
      && readTurnId(message) === incomingTurnId
      && String(message.id || '').startsWith('temp_')
    ))
    : -1;
  if (optimisticIndex >= 0) {
    nextMessages[optimisticIndex] = mergeRealtimeMessage(nextMessages[optimisticIndex], incoming);
    return nextMessages;
  }

  nextMessages.push(incoming);
  return nextMessages;
};

const applyTaskRunnerCallbackRealtimeUpdate = (
  state: ChatStoreDraft,
  sessionId: string,
  persistedUserMessage: Message | null,
  persistedAssistantMessage: Message | null,
) => {
  const turnId = readTurnId(persistedUserMessage) || readTurnId(persistedAssistantMessage);
  if (state.currentSessionId === sessionId) {
    let nextMessages = Array.isArray(state.messages) ? [...state.messages] : [];
    nextMessages = upsertRealtimeMessage(nextMessages, persistedUserMessage);
    nextMessages = upsertRealtimeMessage(nextMessages, persistedAssistantMessage);
    state.messages = nextMessages;
  }

  const cachedEntry = state.sessionMessagesCache?.[sessionId];
  if (cachedEntry && Array.isArray(cachedEntry.messages)) {
    let nextMessages = [...cachedEntry.messages];
    nextMessages = upsertRealtimeMessage(nextMessages, persistedUserMessage);
    nextMessages = upsertRealtimeMessage(nextMessages, persistedAssistantMessage);
    cachedEntry.messages = nextMessages;
  }

  const prev = state.sessionChatState?.[sessionId] || createDefaultSessionChatState();
  const isTerminalPlannerMessage = Boolean(persistedUserMessage || persistedAssistantMessage);
  if (isTerminalPlannerMessage) {
    state.sessionChatState[sessionId] = {
      ...prev,
      isLoading: false,
      isStreaming: false,
      isStopping: false,
      streamingPhase: null,
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
  } else if (turnId && prev.activeTurnId === turnId) {
    state.sessionChatState[sessionId] = {
      ...prev,
      activeTurnId: null,
      isLoading: false,
      isStreaming: false,
      streamingTransport: null,
    };
  }
};

const applyTaskRunnerRealtimeError = (
  state: ChatStoreDraft,
  sessionId: string,
  message: string | null,
) => {
  const prev = state.sessionChatState?.[sessionId] || createDefaultSessionChatState();
  state.sessionChatState[sessionId] = {
    ...prev,
    isLoading: false,
    isStreaming: false,
    isStopping: false,
    streamingPhase: null,
    streamingMessageId: null,
    activeTurnId: null,
    streamingPreviewText: '',
    streamingTransport: null,
  };
  if (state.currentSessionId === sessionId) {
    state.isLoading = false;
    state.isStreaming = false;
    state.streamingMessageId = null;
    if (message) {
      state.error = message;
    }
  }
};

const collectPendingPlannerSessionIds = (
  sessionChatState: ChatStoreDraft['sessionChatState'] | null | undefined,
): string[] => (
  Object.entries(sessionChatState || {})
    .filter(([sessionId, chatState]) => (
      sessionId.trim().length > 0
      && (
        chatState?.isLoading === true
        || Boolean(String(chatState?.activeTurnId || '').trim())
      )
    ))
    .map(([sessionId]) => sessionId)
    .sort()
);

const normalizeRealtimePersistedMessages = (
  payload: Parameters<Parameters<typeof useConversationChatStreamRealtime>[0]['onEvent']>[0],
): {
  sessionId: string;
  persistedUserMessage: Message | null;
  persistedAssistantMessage: Message | null;
} => {
  const sessionId = String(payload.conversation_id || '').trim();
  const rawResult = payload.raw?.result && typeof payload.raw.result === 'object'
    ? payload.raw.result
    : null;
  const persistedUserRaw = rawResult?.persisted_user_message
    ?? payload.raw?.persisted_user_message;
  const persistedAssistantRaw = rawResult?.persisted_assistant_message
    ?? payload.raw?.persisted_assistant_message;
  return {
    sessionId,
    persistedUserMessage: sessionId
      ? normalizePersistedMessage(persistedUserRaw, sessionId)
      : null,
    persistedAssistantMessage: sessionId
      ? normalizePersistedMessage(persistedAssistantRaw, sessionId)
      : null,
  };
};

export const useChatStreamRealtimeBridge = () => {
  const store = useChatStoreContext();
  const realtimeConnectionState = useRealtimeConnectionState();
  const pendingPlannerSessionIds = useChatStoreSelector((state) => (
    collectPendingPlannerSessionIds(state.sessionChatState)
  ), shallow);
  const currentSessionId = useChatStoreSelector((state) => state.currentSessionId || null);
  const chatStoreSet = useMemo<ChatStoreSet>(
    () => ((fn) => {
      store.setState((state) => {
        fn(state as ChatStoreDraft);
      });
    }),
    [store],
  );

  const subscribedConversationIds = useMemo(() => {
    const ids = new Set<string>(pendingPlannerSessionIds);
    if (currentSessionId) {
      ids.add(currentSessionId);
    }
    return Array.from(ids);
  }, [currentSessionId, pendingPlannerSessionIds]);

  const enabled = useMemo(
    () => realtimeConnectionState === 'connected',
    [realtimeConnectionState],
  );

  useRealtimeTopics(
    subscribedConversationIds.map((sessionId) => ({ scope: 'conversation' as const, id: sessionId })),
    enabled && subscribedConversationIds.length > 0,
  );

  useConversationChatStreamRealtime({
    enabled,
    onEvent: async (payload) => {
      const {
        sessionId,
        persistedUserMessage,
        persistedAssistantMessage,
      } = normalizeRealtimePersistedMessages(payload);
      if (!sessionId) {
        return;
      }

      if (persistedUserMessage || persistedAssistantMessage) {
        const state = store.getState();
        chatStoreSet((draft) => {
          applyTaskRunnerCallbackRealtimeUpdate(
            draft,
            sessionId,
            persistedUserMessage,
            persistedAssistantMessage,
          );
        });
        void state.syncSessionMessagesInBackground(sessionId);
        return;
      }

      const eventType = String(payload.raw?.type || payload.stream_type || '').trim().toLowerCase();
      if (eventType === 'error' || eventType === 'cancelled') {
        chatStoreSet((draft) => {
          applyTaskRunnerRealtimeError(
            draft,
            sessionId,
            typeof payload.raw?.message === 'string' ? payload.raw.message : null,
          );
        });
      }
    },
  });
};
