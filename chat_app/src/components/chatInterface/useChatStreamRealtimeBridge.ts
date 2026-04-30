import { useEffect, useMemo, useRef } from 'react';
import { shallow } from 'zustand/shallow';

import {
  useRealtimeConnectionState,
  useRealtimeTopics,
} from '../../lib/realtime/RealtimeProvider';
import { useConversationChatStreamRealtime } from '../../lib/realtime/useConversationChatStreamRealtime';
import type { RealtimeChatStreamPayloadWrapper } from '../../lib/realtime/types';
import { handleStreamEvent } from '../../lib/store/actions/sendMessage/streamEventHandler';
import {
  failSendMessageState,
  finalizeStreamingSessionState,
} from '../../lib/store/actions/sendMessage/sessionState';
import { createStreamingMessageStateHelpers } from '../../lib/store/actions/sendMessage/streamingState';
import type { StreamingMessage } from '../../lib/store/actions/sendMessage/types';
import { buildSendMessageFailure } from '../../lib/store/actions/sendMessage/streamControlEvents';
import {
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
    const { syncSessionMessagesInBackground } = store.getState();
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

      void syncSessionMessagesInBackground(sessionId)
        .catch((error) => {
          console.error('Failed to recover streaming session after realtime disconnect:', error);
        })
        .finally(() => {
          disconnectRecoveryInflightRef.current.delete(sessionId);
        });
    });
  }, [activeStreamingSessionIds, realtimeConnectionState, store]);

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
            chatStoreSet((state) => {
              finalizeStreamingSessionState(state, {
                sessionId: active.sessionId,
                assistantMessageId: active.tempAssistantMessageId,
                sawDone: true,
              });
            });
            const latestState = store.getState();
            if (latestState.currentSessionId === active.sessionId) {
              void latestState.loadMessages(active.sessionId).catch((error) => {
                console.error('Failed to reload messages after realtime cancellation:', error);
              });
            }
          }
        }

        if (result.sawDone) {
          const completionKey = `${active.sessionId}:${active.tempAssistantMessageId}:${payload.stream_type}:${payload.raw.timestamp || ''}`;
          if (!processedCompletionKeysRef.current.has(completionKey)) {
            processedCompletionKeysRef.current.add(completionKey);
            chatStoreSet((state) => {
              finalizeStreamingSessionState(state, {
                sessionId: active.sessionId,
                assistantMessageId: active.tempAssistantMessageId,
                sawDone: true,
              });
            });
            const latestState = store.getState();
            if (latestState.currentSessionId === active.sessionId) {
              void latestState.loadMessages(active.sessionId).catch((error) => {
                console.error('Failed to reload messages after realtime completion:', error);
              });
            }
          }
        }
      } catch (error) {
        const completionKey = `${active.sessionId}:${active.tempAssistantMessageId}:error:${payload.raw.timestamp || ''}`;
        if (!processedCompletionKeysRef.current.has(completionKey)) {
          processedCompletionKeysRef.current.add(completionKey);
          const { failureContent, readableError } = buildSendMessageFailure(
            error,
            active.streamedTextRef.value,
          );
          chatStoreSet((state) => {
            failSendMessageState(state, {
              sessionId: active.sessionId,
              tempAssistantId: active.tempAssistantMessageId,
              tempAssistantMessage,
              failureContent,
              readableError,
            });
          });
        }
      }
    },
  });
};
