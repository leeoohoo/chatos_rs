// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type ApiClient from '../../api/client';
import { fetchSessionMessages } from '../helpers/messages';
import type {
  ChatStoreGet,
  ChatStoreSet,
  SessionMessagePaginationState,
} from '../types';
import { SESSION_MESSAGES_INITIAL_PAGE_SIZE } from './sessionsUtils';

interface LoadingDeps {
  set: ChatStoreSet;
  get: ChatStoreGet;
  client: ApiClient;
}

export function createMessageLoadingActions({ set, get, client }: LoadingDeps) {
  const writePaginationState = (
    target: Record<string, SessionMessagePaginationState>,
    sessionId: string,
    nextBefore: string | null,
  ) => {
    target[sessionId] = {
      nextBefore,
      loaded: true,
    };
  };

  const replaceCurrentSessionMessages = (
    sessionId: string,
    result: Awaited<ReturnType<typeof fetchSessionMessages>>,
    settleGlobalLoading: boolean,
  ) => {
    set((state) => {
      if (state.currentSessionId !== sessionId) {
        return;
      }
      state.messages = result.messages;
      state.hasMoreMessages = Boolean(result.nextBefore);
      if (!state.sessionMessagePaginationState) {
        state.sessionMessagePaginationState = {};
      }
      writePaginationState(
        state.sessionMessagePaginationState,
        sessionId,
        result.nextBefore,
      );
      if (settleGlobalLoading) {
        state.isLoading = false;
      }
    });
  };

  return {
    loadMessages: async (sessionId: string) => {
      try {
        set((state) => {
          state.isLoading = true;
          state.error = null;
        });

        const result = await fetchSessionMessages(client, sessionId, {
          limit: SESSION_MESSAGES_INITIAL_PAGE_SIZE,
          before: null,
        });
        replaceCurrentSessionMessages(sessionId, result, true);
      } catch (error) {
        console.error('Failed to load messages:', error);
        set((state) => {
          if (state.currentSessionId !== sessionId) {
            return;
          }
          state.error = error instanceof Error ? error.message : 'Failed to load messages';
          state.isLoading = false;
        });
      }
    },

    // Public action name retained for existing callers. This performs a direct
    // server refresh and never reads, writes, or merges a message cache.
    syncSessionMessagesInBackground: async (sessionId: string) => {
      const normalizedSessionId = String(sessionId || '').trim();
      if (!normalizedSessionId || get().currentSessionId !== normalizedSessionId) {
        return;
      }
      try {
        const result = await fetchSessionMessages(client, normalizedSessionId, {
          limit: SESSION_MESSAGES_INITIAL_PAGE_SIZE,
          before: null,
        });
        replaceCurrentSessionMessages(normalizedSessionId, result, false);
      } catch (error) {
        console.error('Failed to refresh session messages:', error);
      }
    },

    loadMoreMessages: async (sessionId: string) => {
      try {
        const current = get();
        if (current.currentSessionId !== sessionId) {
          return;
        }
        const before = current.sessionMessagePaginationState?.[sessionId]?.nextBefore ?? null;
        if (!before) {
          set((state) => {
            if (state.currentSessionId === sessionId) {
              state.hasMoreMessages = false;
            }
          });
          return;
        }
        const result = await fetchSessionMessages(client, sessionId, {
          limit: SESSION_MESSAGES_INITIAL_PAGE_SIZE,
          before,
        });
        set((state) => {
          if (state.currentSessionId !== sessionId) {
            return;
          }
          if (!state.sessionMessagePaginationState) {
            state.sessionMessagePaginationState = {};
          }
          const existingIds = new Set(state.messages.map((message) => message.id));
          const older = result.messages.filter((message) => !existingIds.has(message.id));
          state.messages = [...older, ...state.messages];
          writePaginationState(
            state.sessionMessagePaginationState,
            sessionId,
            result.nextBefore,
          );
          state.hasMoreMessages = Boolean(result.nextBefore);
        });
      } catch (error) {
        console.error('Failed to load more messages:', error);
        set((state) => {
          if (state.currentSessionId === sessionId) {
            state.error = error instanceof Error ? error.message : 'Failed to load more messages';
          }
        });
      }
    },
  };
}
