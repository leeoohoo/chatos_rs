import { useEffect, useMemo, useRef } from 'react';
import { shallow } from 'zustand/shallow';

import {
  getRealtimeConnectionStateSnapshot,
  useRealtimeConnectionState,
  useRealtimeTopics,
} from '../../lib/realtime/RealtimeProvider';
import { useConversationChatStreamRealtime } from '../../lib/realtime/useConversationChatStreamRealtime';
import { useApiClient } from '../../lib/api/ApiClientContext';
import {
  shouldRecoverStreamingSessionAfterDisconnect,
} from '../../lib/store/actions/sendMessage/persistedTurnMessages';
import { recoverStreamingTurnBySnapshot } from '../../lib/store/actions/sendMessage/turnRecovery';
import {
  useChatStoreContext,
  useChatStoreSelector,
} from '../../lib/store/ChatStoreContext';
import type {
  ChatStoreDraft,
  ChatStoreSet,
} from '../../lib/store/types';
import {
  collectActiveStreamingSessionIds,
  resolveActiveStreamContext,
  shouldAttemptDisconnectRecovery,
} from './chatStreamRealtimeBridgeState';
import { handleChatStreamRealtimeCompletion } from './chatStreamRealtimeCompletion';

const DISCONNECT_RECOVERY_COOLDOWN_MS = 4000;
const DISCONNECT_RECOVERY_GRACE_MS = 1800;

export const useChatStreamRealtimeBridge = () => {
  const store = useChatStoreContext();
  const apiClient = useApiClient();
  const realtimeConnectionState = useRealtimeConnectionState();
  const activeStreamingSessionIds = useChatStoreSelector((state) => (
    collectActiveStreamingSessionIds(state.sessionChatState)
  ), shallow);
  const currentSessionId = useChatStoreSelector((state) => state.currentSessionId || null);
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

  const subscribedConversationIds = useMemo(() => {
    const ids = new Set<string>(activeStreamingSessionIds);
    if (currentSessionId) {
      ids.add(currentSessionId);
    }
    return Array.from(ids);
  }, [activeStreamingSessionIds, currentSessionId]);

  const enabled = useMemo(
    () => realtimeConnectionState === 'connected',
    [realtimeConnectionState],
  );

  useRealtimeTopics(
    subscribedConversationIds.map((sessionId) => ({ scope: 'conversation' as const, id: sessionId })),
    enabled && subscribedConversationIds.length > 0,
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
          || !shouldAttemptDisconnectRecovery(
            latest as Pick<ChatStoreDraft, 'sessionChatState' | 'sessionStreamingMessageDrafts'>,
            sessionId,
            getRealtimeConnectionStateSnapshot(),
          )
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
      if (payload.stream_type === 'task_runner_callback') {
        const payloadSessionId = String(payload.conversation_id || '').trim();
        const state = store.getState();
        if (payloadSessionId && state.currentSessionId === payloadSessionId) {
          await state.syncSessionMessagesInBackground(payloadSessionId);
        }
        return;
      }
      await handleChatStreamRealtimeCompletion({
        payload,
        storeGetState: () => store.getState() as ChatStoreDraft,
        chatStoreSet,
        apiClient,
        processedCompletionKeysRef,
      });
    },
  });
};
