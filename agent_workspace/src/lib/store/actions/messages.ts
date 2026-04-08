import type { Message } from '../../../types';
import type ApiClient from '../../api/client';
import type {
  ChatStoreGet,
  ChatStoreSet,
} from '../types';
import { createMessageLoadingActions } from './messagesLoading';
import { createTurnProcessActions } from './messagesTurnProcess';

interface Deps {
  set: ChatStoreSet;
  get: ChatStoreGet;
  client: ApiClient;
}


export function createMessageActions({ set, get, client }: Deps) {
  const loadingActions = createMessageLoadingActions({ set, get, client });
  const turnProcessActions = createTurnProcessActions({ set, get, client });

  return {
    ...loadingActions,
    ...turnProcessActions,

    updateMessage: async (messageId: string, _updates: Partial<Message>) => {
      try {
        const updatedMessage = null;

        set((state) => {
          const index = state.messages.findIndex((message) => message.id === messageId);
          if (index !== -1 && updatedMessage) {
            state.messages[index] = updatedMessage;
          }
        });
      } catch (error) {
        console.error('Failed to update message:', error);
        set((state) => {
          state.error = error instanceof Error ? error.message : 'Failed to update message';
        });
      }
    },

    deleteMessage: async (messageId: string) => {
      try {
        set((state) => {
          state.messages = state.messages.filter((message) => message.id !== messageId);
        });
      } catch (error) {
        console.error('Failed to delete message:', error);
        set((state) => {
          state.error = error instanceof Error ? error.message : 'Failed to delete message';
        });
      }
    },
  };
}
