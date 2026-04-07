import type ApiClient from '../../api/client';
import {
  applyTurnProcessCache,
  fetchSessionMessages,
} from '../helpers/messages';
import type {
  ChatStoreGet,
  ChatStoreSet,
} from '../types';
import {
  countLoadedBaseMessages,
  ensureSessionTurnMaps,
  mergeMessagesWithStreamingDraft,
} from './messagesState';

interface LoadingDeps {
  set: ChatStoreSet;
  get: ChatStoreGet;
  client: ApiClient;
}

export function createMessageLoadingActions({ set, get, client }: LoadingDeps) {
  return {
    loadMessages: async (
      sessionId: string,
      options: { silent?: boolean } = {},
    ) => {
      try {
        const session = get().currentSession?.id === sessionId
          ? get().currentSession
          : (get().sessions || []).find((item: any) => item?.id === sessionId) || null;

        if (!options.silent) {
          set((state) => {
            state.isLoading = true;
            state.error = null;
          });
        }

        const messages = await fetchSessionMessages(client, sessionId, {
          limit: 50,
          offset: 0,
          session,
        });

        set((state) => {
          ensureSessionTurnMaps(state, sessionId);

          const nextMessages = mergeMessagesWithStreamingDraft(state, sessionId, messages);
          state.messages = applyTurnProcessCache(
            nextMessages,
            state.sessionTurnProcessCache?.[sessionId],
            state.sessionTurnProcessState?.[sessionId],
          );
          if (!options.silent) {
            state.isLoading = false;
          }
          state.hasMoreMessages = messages.length >= 50;
        });
      } catch (error) {
        console.error('Failed to load messages:', error);
        if (!options.silent) {
          set((state) => {
            state.error = error instanceof Error ? error.message : 'Failed to load messages';
            state.isLoading = false;
          });
        }
      }
    },

    loadMoreMessages: async (sessionId: string) => {
      try {
        const current = get();
        const offset = countLoadedBaseMessages(current.messages);
        const session = current.currentSession?.id === sessionId
          ? current.currentSession
          : (current.sessions || []).find((item: any) => item?.id === sessionId) || null;
        const page = await fetchSessionMessages(client, sessionId, {
          limit: 50,
          offset,
          session,
        });
        set((state) => {
          ensureSessionTurnMaps(state, sessionId);

          const existingIds = new Set(state.messages.map((message) => message.id));
          const older = page.filter((message) => !existingIds.has(message.id));
          const merged = [...older, ...state.messages];

          state.messages = applyTurnProcessCache(
            merged,
            state.sessionTurnProcessCache?.[sessionId],
            state.sessionTurnProcessState?.[sessionId],
          );
          state.hasMoreMessages = page.length >= 50;
        });
      } catch (error) {
        console.error('Failed to load more messages:', error);
        set((state) => {
          state.error = error instanceof Error ? error.message : 'Failed to load more messages';
        });
      }
    },
  };
}
