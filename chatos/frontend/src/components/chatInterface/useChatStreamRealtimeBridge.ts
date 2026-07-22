// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useMemo } from 'react';
import { shallow } from 'zustand/shallow';

import {
  useRealtimeConnectionState,
  useRealtimeTopics,
} from '../../lib/realtime/RealtimeProvider';
import { useConversationChatStreamRealtime } from '../../lib/realtime/useConversationChatStreamRealtime';
import {
  formatAssistantFailureContent,
  sanitizeUserVisibleAiError,
} from '../../lib/store/actions/sendMessage/errorParsing';
import { normalizePersistedMessage } from '../../lib/store/actions/sendMessage/persistedTurnMessages';
import { createDefaultSessionChatState } from '../../lib/store/actions/sendMessage/sessionState';
import { useChatStoreContext, useChatStoreSelector } from '../../lib/store/ChatStoreContext';
import type { Message } from '../../types';
import type {
  ChatStoreDraft,
  ChatStoreSet,
} from '../../lib/store/types';
import { useCurrentTaskRunnerCallbackReconciliation } from './useCurrentTaskRunnerCallbackReconciliation';

import {
  isCancelledEventType,
  isTerminalErrorEventType,
  normalizeEventType,
  readRealtimeErrorMessage,
  readString,
  readTurnId,
  sanitizeMessageIdPart,
} from './chatStreamRealtimeEvent';

const markUserMessageTurnFailed = (
  messages: Message[],
  turnId: string,
  status: 'failed' | 'cancelled',
): Message[] => {
  if (!turnId) {
    return messages;
  }
  return messages.map((message) => {
    if (message.role !== 'user' || readTurnId(message) !== turnId) {
      return message;
    }
    const metadata = message.metadata || {};
    const taskRunnerAsync = metadata.task_runner_async && typeof metadata.task_runner_async === 'object'
      ? metadata.task_runner_async
      : {};
    return {
      ...message,
      metadata: {
        ...metadata,
        task_runner_async: {
          ...taskRunnerAsync,
          mode: 'contact_async',
          overall_status: status,
        },
      },
    };
  });
};

const buildRealtimeFailureMessage = (
  sessionId: string,
  turnId: string,
  message: string,
): Message => {
  const normalizedTurnId = turnId || `unknown_${Date.now()}`;
  const readableError = sanitizeUserVisibleAiError(message || 'Chat turn failed');
  const content = formatAssistantFailureContent(readableError, '');
  return {
    id: `realtime_error_${sanitizeMessageIdPart(sessionId)}_${sanitizeMessageIdPart(normalizedTurnId)}`,
    sessionId,
    role: 'assistant',
    content,
    status: 'error',
    createdAt: new Date(),
    metadata: {
      ...(turnId ? { conversation_turn_id: turnId } : {}),
      contentSegments: [{ content, type: 'text' }],
      currentSegmentIndex: 0,
      requestError: readableError,
    },
  };
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
      clientOptimistic: false,
      clientPendingSync: true,
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

export const applyTaskRunnerCallbackRealtimeUpdate = (
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
  const activeTurnId = readString(prev.activeTurnId);
  const terminalTurnMatchesActive = !activeTurnId || (Boolean(turnId) && turnId === activeTurnId);
  const hasTerminalAssistantMessage = Boolean(persistedAssistantMessage);
  if (hasTerminalAssistantMessage && terminalTurnMatchesActive) {
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
  }
};

export const applyTaskRunnerRealtimeError = (
  state: ChatStoreDraft,
  sessionId: string,
  message: string | null,
  turnId: string,
  eventType: string,
) => {
  const isCancelled = isCancelledEventType(eventType);
  const terminalStatus = isCancelled ? 'cancelled' : 'failed';
  const readableMessage = sanitizeUserVisibleAiError(
    message || (isCancelled ? 'Chat turn cancelled' : 'Chat turn failed'),
  );
  const failureMessage = isCancelled
    ? null
    : buildRealtimeFailureMessage(sessionId, turnId, readableMessage);
  if (state.currentSessionId === sessionId) {
    state.messages = markUserMessageTurnFailed(state.messages || [], turnId, terminalStatus);
    state.messages = upsertRealtimeMessage(state.messages || [], failureMessage);
  }

  const cachedEntry = state.sessionMessagesCache?.[sessionId];
  if (cachedEntry && Array.isArray(cachedEntry.messages)) {
    cachedEntry.messages = markUserMessageTurnFailed(cachedEntry.messages, turnId, terminalStatus);
    cachedEntry.messages = upsertRealtimeMessage(cachedEntry.messages, failureMessage);
  }

  const prev = state.sessionChatState?.[sessionId] || createDefaultSessionChatState();
  const activeTurnId = readString(prev.activeTurnId);
  const terminalTurnMatchesActive = !activeTurnId || (Boolean(turnId) && turnId === activeTurnId);
  if (terminalTurnMatchesActive) {
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
      state.error = isCancelled ? null : readableMessage;
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
  useCurrentTaskRunnerCallbackReconciliation();

  useRealtimeTopics(
    subscribedConversationIds.map((sessionId) => ({ scope: 'conversation' as const, id: sessionId })),
    enabled && subscribedConversationIds.length > 0,
  );

  useConversationChatStreamRealtime({
    enabled,
    onEvent: async (payload, eventName) => {
      const {
        sessionId,
        persistedUserMessage,
        persistedAssistantMessage,
      } = normalizeRealtimePersistedMessages(payload);
      if (!sessionId) {
        return;
      }

      const eventType = normalizeEventType(payload, eventName);
      const isTerminalError = isTerminalErrorEventType(eventType);
      const turnId = readString(payload.conversation_turn_id)
        || readTurnId(persistedUserMessage)
        || readTurnId(persistedAssistantMessage);

      if (persistedUserMessage || persistedAssistantMessage) {
        const state = store.getState();
        chatStoreSet((draft) => {
          applyTaskRunnerCallbackRealtimeUpdate(
            draft,
            sessionId,
            persistedUserMessage,
            persistedAssistantMessage,
          );
          if (isTerminalError && !persistedAssistantMessage) {
            applyTaskRunnerRealtimeError(
              draft,
              sessionId,
              readRealtimeErrorMessage(payload),
              turnId,
              eventType,
            );
          }
        });
        void state.syncSessionMessagesInBackground(sessionId);
        return;
      }

      if (isTerminalError) {
        chatStoreSet((draft) => {
          applyTaskRunnerRealtimeError(
            draft,
            sessionId,
            readRealtimeErrorMessage(payload),
            turnId,
            eventType,
          );
        });
      }
    },
  });
};
