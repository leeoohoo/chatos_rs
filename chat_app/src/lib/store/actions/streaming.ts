import type ApiClient from '../../api/client';
import { debugLog } from '@/lib/utils';
import { createDefaultSessionChatState } from './sendMessage/sessionState';
import type {
  ChatStoreGet,
  ChatStoreSet,
} from '../types';

interface Deps {
  set: ChatStoreSet;
  get: ChatStoreGet;
  client: ApiClient;
}

export function createStreamingActions({ set, get, client }: Deps) {
  return {
    startStreaming: (messageId: string) => {
      set((state) => {
        const sessionId = state.currentSessionId;
        if (sessionId) {
          const prev = state.sessionChatState[sessionId] || createDefaultSessionChatState();
          state.sessionChatState[sessionId] = {
            ...prev,
            isStreaming: true,
            isStopping: false,
            streamingMessageId: messageId,
            streamingPreviewText: '',
          };
        }
        state.isStreaming = true;
        state.streamingMessageId = messageId;
      });
    },

    updateStreamingMessage: (content: string) => {
      set((state) => {
        if (state.streamingMessageId) {
          const messageIndex = state.messages.findIndex((message) => message.id === state.streamingMessageId);
          if (messageIndex !== -1) {
            state.messages[messageIndex].content = content;
          }
        }
      });
    },

    stopStreaming: () => {
      set((state) => {
        const sessionId = state.currentSessionId;
        if (sessionId) {
          const prev = state.sessionChatState[sessionId] || createDefaultSessionChatState();
          state.sessionChatState[sessionId] = {
            ...prev,
            isLoading: false,
            isStreaming: false,
            isStopping: false,
            streamingMessageId: null,
            activeTurnId: null,
            streamingPreviewText: '',
          };
        }
        state.isStreaming = false;
        state.streamingMessageId = null;
      });
    },

    abortCurrentConversation: async () => {
      const { currentSessionId } = get();
      if (!currentSessionId) {
        return;
      }
      const currentSessionState = get().sessionChatState?.[currentSessionId];
      if (currentSessionState?.isStopping) {
        return;
      }

      // 用户点击“停止”后保持会话在流式/加载态，直到后端 cancel 事件或流真正结束，
      // 避免按钮过早恢复为“发送”导致并发发送。
      set((state) => {
        const sessionId = state.currentSessionId;
        if (!sessionId) {
          return;
        }
        const prev = state.sessionChatState[sessionId] || createDefaultSessionChatState();
        if (prev.isStopping) {
          return;
        }
        state.sessionChatState[sessionId] = { ...prev, isLoading: true, isStreaming: true, isStopping: true };
        if (state.currentSessionId === sessionId) {
          state.isLoading = true;
          state.isStreaming = true;
          state.streamingMessageId = prev.streamingMessageId || state.streamingMessageId || null;
        }
      });

      try {
        const {
          selectedModelId,
          aiModelConfigs,
          sessionAiSelectionBySession,
        } = get();
        const sessionAiSelection = sessionAiSelectionBySession?.[currentSessionId];
        const effectiveSelectedModelId = sessionAiSelection?.selectedModelId ?? selectedModelId;
        const activeModel = effectiveSelectedModelId
          ? aiModelConfigs.find((model) => model.id === effectiveSelectedModelId)
          : null;
        const useResponses = activeModel?.supports_responses === true;
        await client.stopChat(currentSessionId, { useResponses });
        debugLog('✅ 成功停止当前对话');
      } catch (error) {
        console.error('❌ 停止对话失败:', error);
        // 停止请求失败时允许用户再次点击“停止”，但仍保持流式态，继续阻止新消息发送。
        set((state) => {
          const sessionId = state.currentSessionId;
          if (!sessionId || sessionId !== currentSessionId) {
            return;
          }
          const prev = state.sessionChatState[sessionId] || createDefaultSessionChatState();
          state.sessionChatState[sessionId] = { ...prev, isStopping: false };
        });
      }
    },
  };
}
