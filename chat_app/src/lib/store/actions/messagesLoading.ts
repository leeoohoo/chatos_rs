import type ApiClient from '../../api/client';
import { fetchSessionMessages } from '../helpers/messages';
import type {
  ChatStoreGet,
  ChatStoreSet,
  SessionMessagePaginationState,
} from '../types';
import {
  extractCompactHistoryMessages,
  mergeLatestCompactHistorySnapshot,
  readSessionMessagesCache,
  readVisibleSessionMessagesSnapshot,
  SESSION_MESSAGES_INITIAL_PAGE_SIZE,
  cloneStreamingMessageDraft,
  writeSessionMessagesCache,
} from './sessionsUtils';

interface LoadingDeps {
  set: ChatStoreSet;
  get: ChatStoreGet;
  client: ApiClient;
}

export function createMessageLoadingActions({ set, get, client }: LoadingDeps) {
  const backgroundSyncInflight = new Map<string, Promise<void>>();
  const writePaginationState = (
    target: Record<string, SessionMessagePaginationState>,
    sessionId: string,
    result: Awaited<ReturnType<typeof fetchSessionMessages>>,
    loaded: boolean,
  ) => {
    target[sessionId] = {
      nextBefore: result.nextBefore,
      loaded,
    };
  };

  const applySessionMessagesSnapshot = (
    sessionId: string,
    result: Awaited<ReturnType<typeof fetchSessionMessages>>,
    options: {
      updateVisibleMessages: boolean;
      settleGlobalLoading: boolean;
    },
    ) => {
    const { messages } = result;
    const visibleSnapshot = readVisibleSessionMessagesSnapshot(get(), sessionId);
    const preservedSnapshot = visibleSnapshot ?? readSessionMessagesCache(get(), sessionId);
    const mergedSnapshot = mergeLatestCompactHistorySnapshot(
      messages,
      result.nextBefore,
      preservedSnapshot,
    );
    set((state) => {
      if (options.updateVisibleMessages || state.currentSessionId === sessionId) {
        state.messages = mergedSnapshot.messages;
        state.hasMoreMessages = Boolean(mergedSnapshot.nextBefore);
      }
      if (!state.sessionMessagePaginationState) {
        state.sessionMessagePaginationState = {};
      }
      writePaginationState(
        state.sessionMessagePaginationState,
        sessionId,
        {
          ...result,
          nextBefore: mergedSnapshot.nextBefore,
        },
        true,
      );

      if (options.settleGlobalLoading) {
        state.isLoading = false;
      }
    });
    set((state) => {
      writeSessionMessagesCache(state, sessionId, {
        messages: mergedSnapshot.messages,
        nextBefore: state.sessionMessagePaginationState?.[sessionId]?.nextBefore ?? mergedSnapshot.nextBefore,
        loaded: true,
      });
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
        applySessionMessagesSnapshot(sessionId, result, {
          updateVisibleMessages: true,
          settleGlobalLoading: true,
        });
      } catch (error) {
        console.error('Failed to load messages:', error);
        set((state) => {
          state.error = error instanceof Error ? error.message : 'Failed to load messages';
          state.isLoading = false;
        });
      }
    },

    syncSessionMessagesInBackground: async (sessionId: string) => {
      const normalizedSessionId = String(sessionId || '').trim();
      if (!normalizedSessionId) {
        return;
      }
      const existingInflight = backgroundSyncInflight.get(normalizedSessionId);
      if (existingInflight) {
        await existingInflight;
        return;
      }
      const request = (async () => {
        try {
          const result = await fetchSessionMessages(client, normalizedSessionId, {
            limit: SESSION_MESSAGES_INITIAL_PAGE_SIZE,
            before: null,
          });
          applySessionMessagesSnapshot(normalizedSessionId, result, {
            updateVisibleMessages: false,
            settleGlobalLoading: false,
          });
        } catch (error) {
          console.error('Failed to sync session messages in background:', error);
        } finally {
          backgroundSyncInflight.delete(normalizedSessionId);
        }
      })();
      backgroundSyncInflight.set(normalizedSessionId, request);
      try {
        await request;
      } catch {
        // request already handled its own errors
      }
    },

    loadMoreMessages: async (sessionId: string) => {
      try {
        const current = get();
        const before = current.sessionMessagePaginationState?.[sessionId]?.nextBefore ?? null;
        if (!before) {
          set((state) => {
            state.hasMoreMessages = false;
          });
          return;
        }
        const result = await fetchSessionMessages(client, sessionId, {
          limit: SESSION_MESSAGES_INITIAL_PAGE_SIZE,
          before,
        });
        const page = result.messages;
        let mergedSnapshotMessages = extractCompactHistoryMessages(current.messages);
        set((state) => {
          if (!state.sessionMessagePaginationState) {
            state.sessionMessagePaginationState = {};
          }

          const existingIds = new Set(state.messages.map((message) => message.id));
          const older = page.filter((message) => !existingIds.has(message.id));
          const merged = [...older, ...state.messages];
          mergedSnapshotMessages = cloneStreamingMessageDraft(extractCompactHistoryMessages(merged));

          state.messages = merged;
          writePaginationState(state.sessionMessagePaginationState, sessionId, result, true);
          state.hasMoreMessages = Boolean(state.sessionMessagePaginationState[sessionId]?.nextBefore);
        });
        set((state) => {
          writeSessionMessagesCache(state, sessionId, {
            messages: mergedSnapshotMessages,
            nextBefore: state.sessionMessagePaginationState?.[sessionId]?.nextBefore ?? result.nextBefore,
            loaded: true,
          });
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
