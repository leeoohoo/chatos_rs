import type ApiClient from '../../api/client';
import { debugLog } from '@/lib/utils';

interface Deps {
  set: any;
  get: any;
  client: ApiClient;
}

export function createStreamingActions({ set, get, client }: Deps) {
  return {
    startStreaming: (messageId: string) => {
      set((state: any) => {
        const sessionId = state.currentSessionId;
        if (sessionId) {
          const prev = state.sessionChatState[sessionId] || { isLoading: false, isStreaming: false, streamingMessageId: null };
          state.sessionChatState[sessionId] = { ...prev, isStreaming: true, streamingMessageId: messageId };
        }
        state.isStreaming = true;
        state.streamingMessageId = messageId;
      });
    },

    updateStreamingMessage: (content: string) => {
      set((state: any) => {
        if (state.streamingMessageId) {
          const messageIndex = state.messages.findIndex((m: any) => m.id === state.streamingMessageId);
          if (messageIndex !== -1) {
            state.messages[messageIndex].content = content;
          }
        }
      });
    },

    stopStreaming: () => {
      set((state: any) => {
        const sessionId = state.currentSessionId;
        if (sessionId) {
          const prev = state.sessionChatState[sessionId] || { isLoading: false, isStreaming: false, streamingMessageId: null };
          state.sessionChatState[sessionId] = { ...prev, isLoading: false, isStreaming: false, streamingMessageId: null };
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

      // 用户点击“停止”后保持会话在流式/加载态，直到后端 cancel 事件或流真正结束，
      // 避免按钮过早恢复为“发送”导致并发发送。
      set((state: any) => {
        const sessionId = state.currentSessionId;
        if (!sessionId) {
          return;
        }
        const prev = state.sessionChatState[sessionId] || { isLoading: false, isStreaming: false, streamingMessageId: null };
        state.sessionChatState[sessionId] = { ...prev, isLoading: true, isStreaming: true };
        if (state.currentSessionId === sessionId) {
          state.isLoading = true;
          state.isStreaming = true;
          state.streamingMessageId = prev.streamingMessageId || state.streamingMessageId || null;
        }
      });

      try {
        const {
          selectedModelId,
          selectedAgentId,
          aiModelConfigs,
          agents,
          sessionAiSelectionBySession,
        } = get();
        const sessionAiSelection = sessionAiSelectionBySession?.[currentSessionId];
        const effectiveSelectedAgentId = sessionAiSelection?.selectedAgentId ?? selectedAgentId;
        const effectiveSelectedModelId = sessionAiSelection?.selectedModelId ?? selectedModelId;
        let activeModel: any = null;
        if (effectiveSelectedAgentId) {
          const agent = agents.find((a: any) => a.id === effectiveSelectedAgentId);
          if (agent) {
            activeModel = aiModelConfigs.find((m: any) => m.id === agent.ai_model_config_id);
          }
        } else if (effectiveSelectedModelId) {
          activeModel = aiModelConfigs.find((m: any) => m.id === effectiveSelectedModelId);
        }
        const useResponses = activeModel?.supports_responses === true;
        await client.stopChat(currentSessionId, { useResponses });
        debugLog('✅ 成功停止当前对话');
      } catch (error) {
        console.error('❌ 停止对话失败:', error);
      }
    },
  };
}
