import type { Message } from '../../../types';
import type ApiClient from '../../api/client';
import {
  applyTurnProcessCache,
  fetchSessionMessages,
  fetchTurnProcessMessages,
  mergeTurnProcessMessages,
  setTurnProcessExpanded,
} from '../helpers/messages';

const cloneStreamingMessageDraft = <T,>(value: T): T => {
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

interface Deps {
  set: any;
  get: any;
  client: ApiClient;
}

const countLoadedBaseMessages = (messages: any[]): number => (
  (messages || []).filter((message: any) => !message?.metadata?.historyProcessUserMessageId).length
);

const getInlineTurnProcessMessages = (messages: any[], userMessageId: string): Message[] => {
  const userMessage = (messages || []).find((message: any) => (
    message?.id === userMessageId && message?.role === 'user'
  ));
  const inlineMessages = userMessage?.metadata?.historyProcessInlineMessages;
  return Array.isArray(inlineMessages) ? inlineMessages : [];
};

const ensureSessionTurnMaps = (state: any, sessionId: string) => {
  if (!state.sessionTurnProcessState) {
    state.sessionTurnProcessState = {};
  }
  if (!state.sessionTurnProcessState[sessionId]) {
    state.sessionTurnProcessState[sessionId] = {};
  }

  if (!state.sessionTurnProcessCache) {
    state.sessionTurnProcessCache = {};
  }
  if (!state.sessionTurnProcessCache[sessionId]) {
    state.sessionTurnProcessCache[sessionId] = {};
  }
};

export function createMessageActions({ set, get, client }: Deps) {
  return {
    loadMessages: async (sessionId: string) => {
      try {
        set((state: any) => {
          state.isLoading = true;
          state.error = null;
        });

        const messages = await fetchSessionMessages(client, sessionId, { limit: 50, offset: 0 });

        set((state: any) => {
          ensureSessionTurnMaps(state, sessionId);

          const chatState = state.sessionChatState?.[sessionId];
          const draftMessage = state.sessionStreamingMessageDrafts?.[sessionId];
          let nextMessages = messages;

          if (chatState?.isStreaming && chatState.streamingMessageId) {
            const hasStreamingMessage = nextMessages.some((m: any) => m.id === chatState.streamingMessageId);
            if (!hasStreamingMessage && draftMessage && typeof draftMessage === 'object') {
              nextMessages = [...nextMessages, cloneStreamingMessageDraft(draftMessage)];
            }
          }

          nextMessages = applyTurnProcessCache(
            nextMessages,
            state.sessionTurnProcessCache?.[sessionId],
            state.sessionTurnProcessState?.[sessionId],
          );

          state.messages = nextMessages;
          state.isLoading = false;
          state.hasMoreMessages = messages.length >= 50;
        });
      } catch (error) {
        console.error('Failed to load messages:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to load messages';
          state.isLoading = false;
        });
      }
    },

    loadMoreMessages: async (sessionId: string) => {
      try {
        const current = get();
        const offset = countLoadedBaseMessages(current.messages);
        const page = await fetchSessionMessages(client, sessionId, { limit: 50, offset });
        set((state: any) => {
          ensureSessionTurnMaps(state, sessionId);

          const existingIds = new Set(state.messages.map((m: any) => m.id));
          const older = page.filter((m: any) => !existingIds.has(m.id));
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
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to load more messages';
        });
      }
    },

    toggleTurnProcess: async (userMessageId: string) => {
      const snapshot = get();
      const sessionId = snapshot.currentSessionId;
      if (!sessionId || !userMessageId) {
        return;
      }

      const currentState = snapshot.sessionTurnProcessState?.[sessionId]?.[userMessageId] || {
        expanded: false,
        loaded: false,
        loading: false,
      };
      const nextExpanded = !currentState.expanded;

      if (nextExpanded && !currentState.loaded && !currentState.loading) {
        const inlineProcessMessages = getInlineTurnProcessMessages(snapshot.messages, userMessageId);
        if (inlineProcessMessages.length > 0) {
          set((state: any) => {
            ensureSessionTurnMaps(state, sessionId);
            state.sessionTurnProcessCache[sessionId][userMessageId] = inlineProcessMessages;
            state.sessionTurnProcessState[sessionId][userMessageId] = {
              expanded: true,
              loaded: true,
              loading: false,
            };

            state.messages = mergeTurnProcessMessages(
              state.messages,
              userMessageId,
              inlineProcessMessages,
              true,
            );
          });
          return;
        }

        set((state: any) => {
          ensureSessionTurnMaps(state, sessionId);
          state.sessionTurnProcessState[sessionId][userMessageId] = {
            expanded: true,
            loaded: false,
            loading: true,
          };
          state.messages = applyTurnProcessCache(
            state.messages,
            state.sessionTurnProcessCache?.[sessionId],
            state.sessionTurnProcessState?.[sessionId],
          );
        });

        try {
          const processMessages = await fetchTurnProcessMessages(client, sessionId, userMessageId);
          set((state: any) => {
            ensureSessionTurnMaps(state, sessionId);
            state.sessionTurnProcessCache[sessionId][userMessageId] = processMessages;
            state.sessionTurnProcessState[sessionId][userMessageId] = {
              expanded: true,
              loaded: true,
              loading: false,
            };

            state.messages = mergeTurnProcessMessages(
              state.messages,
              userMessageId,
              processMessages,
              true,
            );
          });
        } catch (error) {
          console.error('Failed to load turn process messages:', error);
          set((state: any) => {
            ensureSessionTurnMaps(state, sessionId);
            state.sessionTurnProcessState[sessionId][userMessageId] = {
              expanded: false,
              loaded: false,
              loading: false,
            };
            state.messages = applyTurnProcessCache(
              setTurnProcessExpanded(state.messages, userMessageId, false),
              state.sessionTurnProcessCache?.[sessionId],
              state.sessionTurnProcessState?.[sessionId],
            );
            state.error = error instanceof Error ? error.message : 'Failed to load turn process messages';
          });
        }

        return;
      }

      set((state: any) => {
        ensureSessionTurnMaps(state, sessionId);
        state.sessionTurnProcessState[sessionId][userMessageId] = {
          expanded: nextExpanded,
          loaded: currentState.loaded,
          loading: false,
        };

        const toggled = setTurnProcessExpanded(state.messages, userMessageId, nextExpanded);
        state.messages = applyTurnProcessCache(
          toggled,
          state.sessionTurnProcessCache?.[sessionId],
          state.sessionTurnProcessState?.[sessionId],
        );
      });
    },

    updateMessage: async (messageId: string, _updates: Partial<Message>) => {
      try {
        console.warn('updateMessage not implemented yet');
        const updatedMessage = null;

        set((state: any) => {
          const index = state.messages.findIndex((m: any) => m.id === messageId);
          if (index !== -1 && updatedMessage) {
            state.messages[index] = updatedMessage;
          }
        });
      } catch (error) {
        console.error('Failed to update message:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to update message';
        });
      }
    },

    deleteMessage: async (messageId: string) => {
      try {
        console.warn('deleteMessage not implemented yet');

        set((state: any) => {
          state.messages = state.messages.filter((m: any) => m.id !== messageId);
        });
      } catch (error) {
        console.error('Failed to delete message:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to delete message';
        });
      }
    },
  };
}
