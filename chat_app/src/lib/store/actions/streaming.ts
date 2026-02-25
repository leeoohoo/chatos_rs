import type ApiClient from '../../api/client';
import { debugLog } from '@/lib/utils';

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

      if (currentSessionId) {
        try {
          const { selectedModelId, selectedAgentId, aiModelConfigs, agents } = get();
          let activeModel: any = null;
          if (selectedAgentId) {
            const agent = agents.find((a: any) => a.id === selectedAgentId);
            if (agent) {
              activeModel = aiModelConfigs.find((m: any) => m.id === agent.ai_model_config_id);
            }
          } else if (selectedModelId) {
            activeModel = aiModelConfigs.find((m: any) => m.id === selectedModelId);
          }
          const useResponses = activeModel?.supports_responses === true;
          await client.stopChat(currentSessionId, { useResponses });
          debugLog('✅ 成功停止当前对话');
        } catch (error) {
          console.error('❌ 停止对话失败:', error);
        }
      }

      set((state: any) => {
        const sessionId = state.currentSessionId;
        const streamingId = state.streamingMessageId;
        if (sessionId) {
          const currentDraft = state.sessionStreamingMessageDrafts?.[sessionId];
          if (currentDraft) {
            const cancelledDraft = cloneStreamingMessageDraft(currentDraft);
            const metadataToolCalls = (cancelledDraft as any)?.metadata?.toolCalls;
            if (Array.isArray(metadataToolCalls)) {
              metadataToolCalls.forEach((tc: any) => {
                if (!tc.error) {
                  const hasResult = tc.result !== undefined && tc.result !== null && String(tc.result).trim() !== '';
                  if (!hasResult) {
                    tc.result = tc.result || '';
                  }
                  tc.error = '已取消';
                }
                tc.completed = true;
              });
            }
            (cancelledDraft as any).status = 'completed';

            const existingIndex = state.messages.findIndex((m: any) => m.id === (cancelledDraft as any).id);
            const shouldWriteToCurrentMessages = existingIndex !== -1 || state.currentSessionId === sessionId;
            if (existingIndex !== -1) {
              state.messages[existingIndex] = {
                ...state.messages[existingIndex],
                ...cancelledDraft,
              };
            } else if (shouldWriteToCurrentMessages) {
              state.messages.push(cancelledDraft);
            }
            state.sessionStreamingMessageDrafts[sessionId] = null;
          }

          const prev = state.sessionChatState[sessionId] || { isLoading: false, isStreaming: false, streamingMessageId: null };
          state.sessionChatState[sessionId] = { ...prev, isLoading: false, isStreaming: false, streamingMessageId: null };
        }
        state.isStreaming = false;
        state.streamingMessageId = null;
        state.isLoading = false;
        if (streamingId) {
          const messageIndex = state.messages.findIndex((m: any) => m.id === streamingId);
          if (messageIndex !== -1) {
            const message = state.messages[messageIndex];
            if (message.metadata && message.metadata.toolCalls) {
              message.metadata.toolCalls.forEach((tc: any) => {
                if (!tc.error) {
                  const hasResult = tc.result !== undefined && tc.result !== null && String(tc.result).trim() !== '';
                  if (!hasResult) {
                    tc.result = tc.result || '';
                  }
                  tc.error = '已取消';
                }
              });
              (message as any).updatedAt = new Date();
            }
          }
        }
      });
    },
  };
}
