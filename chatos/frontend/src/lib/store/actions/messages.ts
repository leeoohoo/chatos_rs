// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Message } from '../../../types';
import type ApiClient from '../../api/client';
import type {
  ChatStoreGet,
  ChatStoreSet,
} from '../types';
import {
  cloneStreamingMessageDraft,
  extractCompactHistoryMessages,
  writeSessionMessagesCache,
} from './sessionsUtils';
import { createMessageLoadingActions } from './messagesLoading';

interface Deps {
  set: ChatStoreSet;
  get: ChatStoreGet;
  client: ApiClient;
}


export function createMessageActions({ set, get, client }: Deps) {
  const loadingActions = createMessageLoadingActions({ set, get, client });

  return {
    ...loadingActions,

    upsertSessionMessage: (message: Message) => {
      const sessionId = String(message?.sessionId || '').trim();
      const messageId = String(message?.id || '').trim();
      if (!sessionId || !messageId) {
        return;
      }

      const mergeMessages = (messages: Message[] = []): Message[] => {
        const next = [...messages.filter((item) => item.id !== messageId), message];
        return next
          .map((item, index) => ({ item, index }))
          .sort((left, right) => {
            const leftTime = left.item.createdAt instanceof Date ? left.item.createdAt.getTime() : 0;
            const rightTime = right.item.createdAt instanceof Date ? right.item.createdAt.getTime() : 0;
            if (Number.isFinite(leftTime) && Number.isFinite(rightTime) && leftTime !== rightTime) {
              return leftTime - rightTime;
            }
            return left.index - right.index;
          })
          .map(({ item }) => item);
      };

      set((state) => {
        if (state.currentSessionId === sessionId) {
          state.messages = mergeMessages(state.messages || []);
        }

        const cached = state.sessionMessagesCache?.[sessionId];
        const cachedMessages = cached?.messages || [];
        const mergedCachedMessages = mergeMessages(cachedMessages);
        writeSessionMessagesCache(state, sessionId, {
          messages: cloneStreamingMessageDraft(extractCompactHistoryMessages(mergedCachedMessages)),
          nextBefore: state.sessionMessagePaginationState?.[sessionId]?.nextBefore
            ?? cached?.nextBefore
            ?? null,
          loaded: cached?.loaded ?? state.sessionMessagePaginationState?.[sessionId]?.loaded ?? true,
        });
      });
    },

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
