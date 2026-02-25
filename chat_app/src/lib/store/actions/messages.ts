import type { Message } from '../../../types';
import type ApiClient from '../../api/client';
import { fetchSessionMessages } from '../helpers/messages';

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
          const chatState = state.sessionChatState?.[sessionId];
          const draftMessage = state.sessionStreamingMessageDrafts?.[sessionId];
          let nextMessages = messages;

          if (chatState?.isStreaming && chatState.streamingMessageId) {
            const hasStreamingMessage = nextMessages.some((m: any) => m.id === chatState.streamingMessageId);
            if (!hasStreamingMessage && draftMessage && typeof draftMessage === 'object') {
              nextMessages = [...nextMessages, cloneStreamingMessageDraft(draftMessage)];
            }
          }

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
        const offset = current.messages.length;
        const page = await fetchSessionMessages(client, sessionId, { limit: 50, offset });
        set((state: any) => {
          const existingIds = new Set(state.messages.map((m: any) => m.id));
          const older = page.filter((m: any) => !existingIds.has(m.id));
          state.messages = [...older, ...state.messages];
          state.hasMoreMessages = page.length >= 50;
        });
      } catch (error) {
        console.error('Failed to load more messages:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to load more messages';
        });
      }
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
