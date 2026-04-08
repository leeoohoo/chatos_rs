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
  abortSessionChat?: (sessionId: string) => Promise<boolean> | boolean;
}

export function createConversationControlActions({ set, get, client, abortSessionChat }: Deps) {
  return {
    abortCurrentConversation: async () => {
      const { currentSessionId } = get();
      if (!currentSessionId) {
        return;
      }
      const currentSessionState = get().sessionChatState?.[currentSessionId];
      if (currentSessionState?.isStopping) {
        return;
      }

      // 用户点击“停止”后保持会话在运行中，直到后端 cancel/done 事件真正落地，
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
        const handledBySessionWs = await abortSessionChat?.(currentSessionId);
        if (handledBySessionWs) {
          debugLog('✅ 已通过 session websocket 请求停止当前对话');
          return;
        }

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
        // 停止请求失败时允许用户再次点击“停止”，但仍保持运行态，继续阻止新消息发送。
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
