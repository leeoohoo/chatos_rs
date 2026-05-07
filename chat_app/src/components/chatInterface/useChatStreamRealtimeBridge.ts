import { useEffect, useMemo, useRef } from 'react';
import { shallow } from 'zustand/shallow';

import {
  getRealtimeConnectionStateSnapshot,
  useRealtimeConnectionState,
  useRealtimeTopics,
} from '../../lib/realtime/RealtimeProvider';
import { useConversationChatStreamRealtime } from '../../lib/realtime/useConversationChatStreamRealtime';
import type { RealtimeChatStreamPayloadWrapper } from '../../lib/realtime/types';
import { apiClient as globalApiClient } from '../../lib/api/client';
import {
  normalizePersistedMessage,
  reconcilePersistedTurnMessages,
  shouldRecoverStreamingSessionAfterDisconnect,
  shouldReloadMessagesAfterTerminalState,
} from '../../lib/store/actions/sendMessage/persistedTurnMessages';
import { recoverStreamingTurnBySnapshot } from '../../lib/store/actions/sendMessage/turnRecovery';
import { handleStreamEvent } from '../../lib/store/actions/sendMessage/streamEventHandler';
import {
  failSendMessageState,
  finalizeStreamingSessionState,
} from '../../lib/store/actions/sendMessage/sessionState';
import { createStreamingMessageStateHelpers } from '../../lib/store/actions/sendMessage/streamingState';
import type { StreamingMessage } from '../../lib/store/actions/sendMessage/types';
import { buildSendMessageFailure } from '../../lib/store/actions/sendMessage/streamControlEvents';
import {
  useChatApiClientFromContext,
  useChatStoreContext,
  useChatStoreSelector,
} from '../../lib/store/ChatStoreContext';
import type {
  ChatStoreDraft,
  ChatStoreSet,
} from '../../lib/store/types';

interface ActiveStreamContext {
  sessionId: string;
  conversationTurnId: string;
  tempAssistantMessageId: string;
  tempUserId: string | null;
  streamedTextRef: { value: string };
}

const isStreamingMessage = (value: unknown): value is StreamingMessage => (
  Boolean(value && typeof value === 'object' && typeof (value as { id?: unknown }).id === 'string')
);

const DISCONNECT_RECOVERY_COOLDOWN_MS = 4000;
const DISCONNECT_RECOVERY_GRACE_MS = 1800;

const asRealtimeParsedEvent = (payload: RealtimeChatStreamPayloadWrapper) => {
  const raw = payload.raw && typeof payload.raw === 'object'
    ? payload.raw
    : {};
  return {
    ...raw,
    type: typeof raw.type === 'string' ? raw.type : payload.stream_type,
  };
};

const readTrimmedString = (value: unknown): string | null => {
  if (typeof value !== 'string') {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
};

const resolvePayloadConversationTurnId = (
  payload: RealtimeChatStreamPayloadWrapper,
): string | null => {
  const raw = payload.raw && typeof payload.raw === 'object'
    ? payload.raw as Record<string, unknown>
    : null;
  return readTrimmedString(payload.conversation_turn_id)
    || readTrimmedString(raw?.conversation_turn_id)
    || readTrimmedString(raw?.turn_id);
};

const resolveActiveStreamContext = (
  state: ChatStoreDraft,
  sessionId: string,
): ActiveStreamContext | null => {
  const chatState = state.sessionChatState?.[sessionId];
  if (!chatState?.isStreaming || !chatState.streamingMessageId) {
    return null;
  }
  const draft = state.sessionStreamingMessageDrafts?.[sessionId];
  if (!isStreamingMessage(draft)) {
    return null;
  }
  const conversationTurnId = String(
    draft.metadata?.conversation_turn_id
    || chatState.activeTurnId
    || '',
  ).trim();
  if (!conversationTurnId) {
    return null;
  }
  const tempUserId = typeof draft.metadata?.historyFinalForUserMessageId === 'string'
    ? draft.metadata.historyFinalForUserMessageId
    : (
      typeof draft.metadata?.historyDraftUserMessage?.id === 'string'
        ? draft.metadata.historyDraftUserMessage.id
        : null
    );
  return {
    sessionId,
    conversationTurnId,
    tempAssistantMessageId: draft.id,
    tempUserId,
    streamedTextRef: {
      value: typeof draft.content === 'string' ? draft.content : '',
    },
  };
};

export const useChatStreamRealtimeBridge = () => {
  const store = useChatStoreContext();
  const apiClientFromContext = useChatApiClientFromContext();
  const apiClient = apiClientFromContext || globalApiClient;
  const realtimeConnectionState = useRealtimeConnectionState();
  const activeStreamingSessionIds = useChatStoreSelector((state) => (
    Object.entries(state.sessionChatState || {})
      .filter(([sessionId, chatState]) => (
        sessionId.trim().length > 0
        && chatState?.isStreaming === true
        && typeof chatState.streamingMessageId === 'string'
        && chatState.streamingMessageId.trim().length > 0
      ))
      .map(([sessionId]) => sessionId)
      .sort()
  ), shallow);
  const processedCompletionKeysRef = useRef<Set<string>>(new Set());
  const previousConnectionStateRef = useRef(realtimeConnectionState);
  const disconnectRecoveryInflightRef = useRef<Set<string>>(new Set());
  const disconnectRecoveryLastRunAtRef = useRef<Map<string, number>>(new Map());
  const chatStoreSet = useMemo<ChatStoreSet>(
    () => ((fn) => {
      store.setState((state) => {
        fn(state as ChatStoreDraft);
      });
    }),
    [store],
  );

  const enabled = useMemo(
    () => realtimeConnectionState === 'connected',
    [realtimeConnectionState],
  );

  useRealtimeTopics(
    activeStreamingSessionIds.map((sessionId) => ({ scope: 'conversation' as const, id: sessionId })),
    enabled && activeStreamingSessionIds.length > 0,
  );

  useEffect(() => {
    const previousState = previousConnectionStateRef.current;
    previousConnectionStateRef.current = realtimeConnectionState;

    const lostRealtimeAfterConnected = (
      previousState === 'connected'
      && (realtimeConnectionState === 'disconnected' || realtimeConnectionState === 'error')
    );
    if (!lostRealtimeAfterConnected || activeStreamingSessionIds.length === 0) {
      return;
    }

    const now = Date.now();
    activeStreamingSessionIds.forEach((sessionId) => {
      if (disconnectRecoveryInflightRef.current.has(sessionId)) {
        return;
      }
      const lastRunAt = disconnectRecoveryLastRunAtRef.current.get(sessionId) || 0;
      if (now - lastRunAt < DISCONNECT_RECOVERY_COOLDOWN_MS) {
        return;
      }

      disconnectRecoveryInflightRef.current.add(sessionId);
      disconnectRecoveryLastRunAtRef.current.set(sessionId, now);

      window.setTimeout(() => {
        const latest = store.getState();
        const latestChatState = latest.sessionChatState?.[sessionId];
        if (
          !latestChatState?.isStreaming
          || latestChatState.streamingTransport !== 'realtime'
          || getRealtimeConnectionStateSnapshot() === 'connected'
          || !shouldRecoverStreamingSessionAfterDisconnect(latest, sessionId)
        ) {
          disconnectRecoveryInflightRef.current.delete(sessionId);
          return;
        }

        const latestActive = resolveActiveStreamContext(latest as ChatStoreDraft, sessionId);
        if (!latestActive) {
          disconnectRecoveryInflightRef.current.delete(sessionId);
          return;
        }

        void recoverStreamingTurnBySnapshot({
          apiClient,
          set: chatStoreSet,
          sessionId,
          turnId: latestActive.conversationTurnId,
          tempAssistantMessageId: latestActive.tempAssistantMessageId,
          tempUserId: latestActive.tempUserId,
          preferredUserMessageId: latestActive.tempUserId,
        })
          .then((result) => {
            if (result.recovered) {
              return;
            }
            return store.getState().syncSessionMessagesInBackground(sessionId);
          })
          .catch((error) => {
            console.error('Failed to recover streaming session after realtime disconnect:', error);
          })
          .finally(() => {
            disconnectRecoveryInflightRef.current.delete(sessionId);
          });
      }, DISCONNECT_RECOVERY_GRACE_MS);
    });
  }, [activeStreamingSessionIds, apiClient, chatStoreSet, realtimeConnectionState, store]);

  useConversationChatStreamRealtime({
    enabled,
    onEvent: async (payload) => {
      const payloadSessionId = String(payload.conversation_id || '').trim();
      if (!payloadSessionId) {
        return;
      }
      const parsed = asRealtimeParsedEvent(payload);
      const currentState = store.getState();
      const active = resolveActiveStreamContext(currentState, payloadSessionId);
      if (!active) {
        return;
      }
      const payloadTurnId = resolvePayloadConversationTurnId(payload);
      if (payloadTurnId && payloadTurnId !== active.conversationTurnId) {
        return;
      }
      active.streamedTextRef.value = (
        currentState.sessionStreamingMessageDrafts?.[active.sessionId]?.content
        && typeof currentState.sessionStreamingMessageDrafts?.[active.sessionId]?.content === 'string'
      )
        ? String(currentState.sessionStreamingMessageDrafts?.[active.sessionId]?.content || '')
        : active.streamedTextRef.value;

      const tempAssistantMessage = currentState.sessionStreamingMessageDrafts?.[active.sessionId];
      if (!isStreamingMessage(tempAssistantMessage)) {
        return;
      }

      const helpers = createStreamingMessageStateHelpers({
        set: chatStoreSet,
        currentSessionId: active.sessionId,
        tempAssistantMessage,
        tempUserId: active.tempUserId,
        conversationTurnId: active.conversationTurnId,
        streamedTextRef: active.streamedTextRef,
      });

      try {
        const result = handleStreamEvent({
          parsed,
          set: chatStoreSet,
          currentSessionId: active.sessionId,
          conversationTurnId: active.conversationTurnId,
          tempAssistantMessageId: active.tempAssistantMessageId,
          streamedTextRef: active.streamedTextRef,
          helpers,
        });

        if (result.sawCancelled) {
          const cancellationKey = `${active.sessionId}:${active.tempAssistantMessageId}:cancelled:${payload.raw.timestamp || ''}`;
          if (!processedCompletionKeysRef.current.has(cancellationKey)) {
            processedCompletionKeysRef.current.add(cancellationKey);
            const persistedUserMessage = normalizePersistedMessage(
              payload.raw?.result?.persisted_user_message,
              active.sessionId,
            );
            const persistedAssistantMessage = normalizePersistedMessage(
              payload.raw?.result?.persisted_assistant_message,
              active.sessionId,
            );
            chatStoreSet((state) => {
              reconcilePersistedTurnMessages(
                state,
                active.tempAssistantMessageId,
                active.tempUserId,
                persistedUserMessage,
                persistedAssistantMessage,
              );
              finalizeStreamingSessionState(state, {
                sessionId: active.sessionId,
                assistantMessageId: active.tempAssistantMessageId,
                sawDone: true,
              });
            });
            const latestState = store.getState();
            if (
              latestState.currentSessionId === active.sessionId
              && shouldReloadMessagesAfterTerminalState(
                latestState,
                active.tempAssistantMessageId,
                active.tempUserId,
                {
                  allowLocalTerminalAssistant: true,
                },
              )
            ) {
              void recoverStreamingTurnBySnapshot({
                apiClient,
                set: chatStoreSet,
                sessionId: active.sessionId,
                turnId: active.conversationTurnId,
                tempAssistantMessageId: active.tempAssistantMessageId,
                tempUserId: active.tempUserId,
                preferredUserMessageId: active.tempUserId,
              }).then((result) => {
                if (result.recovered) {
                  return;
                }
                return latestState.loadMessages(active.sessionId);
              }).catch((error) => {
                console.error('Failed to recover messages after realtime cancellation:', error);
              });
            }
          }
        }

        if (result.sawDone) {
          const completionKey = `${active.sessionId}:${active.tempAssistantMessageId}:${payload.stream_type}:${payload.raw.timestamp || ''}`;
          if (!processedCompletionKeysRef.current.has(completionKey)) {
            processedCompletionKeysRef.current.add(completionKey);
            const persistedUserMessage = normalizePersistedMessage(
              payload.raw?.result?.persisted_user_message,
              active.sessionId,
            );
            const persistedAssistantMessage = normalizePersistedMessage(
              payload.raw?.result?.persisted_assistant_message,
              active.sessionId,
            );
            chatStoreSet((state) => {
              reconcilePersistedTurnMessages(
                state,
                active.tempAssistantMessageId,
                active.tempUserId,
                persistedUserMessage,
                persistedAssistantMessage,
              );
              finalizeStreamingSessionState(state, {
                sessionId: active.sessionId,
                assistantMessageId: active.tempAssistantMessageId,
                sawDone: true,
              });
            });
            const latestState = store.getState();
            if (
              latestState.currentSessionId === active.sessionId
              && shouldReloadMessagesAfterTerminalState(
                latestState,
                active.tempAssistantMessageId,
                active.tempUserId,
                {
                  allowLocalTerminalAssistant: true,
                },
              )
            ) {
              void recoverStreamingTurnBySnapshot({
                apiClient,
                set: chatStoreSet,
                sessionId: active.sessionId,
                turnId: active.conversationTurnId,
                tempAssistantMessageId: active.tempAssistantMessageId,
                tempUserId: active.tempUserId,
                preferredUserMessageId: active.tempUserId,
              }).then((result) => {
                if (result.recovered) {
                  return;
                }
                return latestState.loadMessages(active.sessionId);
              }).catch((error) => {
                console.error('Failed to recover messages after realtime completion:', error);
              });
            }
          }
        }
      } catch (error) {
        const completionKey = `${active.sessionId}:${active.tempAssistantMessageId}:error:${payload.raw.timestamp || ''}`;
        if (!processedCompletionKeysRef.current.has(completionKey)) {
          processedCompletionKeysRef.current.add(completionKey);
          const persistedUserMessage = normalizePersistedMessage(
            payload.raw?.result?.persisted_user_message,
            active.sessionId,
          );
          const persistedAssistantMessage = normalizePersistedMessage(
            payload.raw?.result?.persisted_assistant_message,
            active.sessionId,
          );
          const { failureContent, readableError } = buildSendMessageFailure(
            error,
            active.streamedTextRef.value,
          );
          chatStoreSet((state) => {
            reconcilePersistedTurnMessages(
              state,
              active.tempAssistantMessageId,
              active.tempUserId,
              persistedUserMessage,
              persistedAssistantMessage,
            );
            failSendMessageState(state, {
              sessionId: active.sessionId,
              tempAssistantId: active.tempAssistantMessageId,
              tempAssistantMessage,
              failureContent,
              readableError,
            });
          });
          const latestState = store.getState();
          if (
            latestState.currentSessionId === active.sessionId
            && shouldReloadMessagesAfterTerminalState(
              latestState,
              active.tempAssistantMessageId,
              active.tempUserId,
              {
                allowLocalTerminalAssistant: true,
              },
            )
          ) {
            void recoverStreamingTurnBySnapshot({
              apiClient,
              set: chatStoreSet,
              sessionId: active.sessionId,
              turnId: active.conversationTurnId,
              tempAssistantMessageId: active.tempAssistantMessageId,
              tempUserId: active.tempUserId,
              preferredUserMessageId: active.tempUserId,
            }).then((result) => {
              if (result.recovered) {
                return;
              }
              return latestState.loadMessages(active.sessionId);
            }).catch((loadError) => {
              console.error('Failed to recover messages after realtime error:', loadError);
            });
          }
        }
      }
    },
  });
};
