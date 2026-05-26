import type ApiClient from '../../api/client';
import { debugLog } from '@/lib/utils';
import { createDefaultSessionChatState } from './sendMessage/sessionState';
import { recoverStreamingTurnBySnapshot } from './sendMessage/turnRecovery';
import type {
  ChatStoreGet,
  ChatStoreSet,
} from '../types';

interface Deps {
  set: ChatStoreSet;
  get: ChatStoreGet;
  client: ApiClient;
}

const STOP_RECOVERY_DELAY_MS = 4000;
const STOP_RECOVERY_MAX_ATTEMPTS = 3;

type StopRecoveryContext = {
  sessionId: string;
  turnId: string;
  assistantMessageId: string;
  tempUserId: string | null;
  preferredUserMessageId: string | null;
};

const readTrimmedString = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

const buildStopRecoveryContext = (
  state: ReturnType<ChatStoreGet>,
  sessionId: string,
): StopRecoveryContext | null => {
  const chatState = state.sessionChatState?.[sessionId];
  const draft = state.sessionStreamingMessageDrafts?.[sessionId];
  const turnId = readTrimmedString(
    draft?.metadata?.conversation_turn_id
    || chatState?.activeTurnId
    || '',
  );
  const assistantMessageId = readTrimmedString(
    draft?.id
    || chatState?.streamingMessageId
    || '',
  );
  if (!turnId || !assistantMessageId) {
    return null;
  }
  const tempUserId = readTrimmedString(
    draft?.metadata?.historyFinalForUserMessageId
    || draft?.metadata?.historyDraftUserMessage?.id
    || '',
  ) || null;
  return {
    sessionId,
    turnId,
    assistantMessageId,
    tempUserId,
    preferredUserMessageId: tempUserId,
  };
};

export function createStreamingActions({ set, get, client }: Deps) {
  const matchesStopRecoveryContext = (context: StopRecoveryContext): boolean => {
    const latest = get();
    const chatState = latest.sessionChatState?.[context.sessionId];
    if (!chatState || (!chatState.isStopping && !chatState.isStreaming)) {
      return false;
    }
    const latestContext = buildStopRecoveryContext(latest, context.sessionId);
    return Boolean(
      latestContext
      && latestContext.turnId === context.turnId
      && latestContext.assistantMessageId === context.assistantMessageId
      && latestContext.tempUserId === context.tempUserId,
    );
  };

  const recoverStoppedConversation = async (
    context: StopRecoveryContext,
    attempt: number,
  ) => {
    if (!matchesStopRecoveryContext(context)) {
      return;
    }

    try {
      const result = await recoverStreamingTurnBySnapshot({
        apiClient: client,
        set,
        sessionId: context.sessionId,
        turnId: context.turnId,
        tempAssistantMessageId: context.assistantMessageId,
        tempUserId: context.tempUserId,
        preferredUserMessageId: context.preferredUserMessageId,
      });
      if (result.recovered && result.terminal) {
        return;
      }
    } catch (error) {
      console.error('Failed to recover stopped conversation by runtime snapshot:', error);
    }

    const latest = get();
    if (!matchesStopRecoveryContext(context)) {
      return;
    }

    if (typeof latest.syncSessionMessagesInBackground === 'function') {
      await latest.syncSessionMessagesInBackground(context.sessionId);
    }

    if (attempt >= STOP_RECOVERY_MAX_ATTEMPTS || !matchesStopRecoveryContext(context)) {
      return;
    }

    setTimeout(() => {
      void recoverStoppedConversation(context, attempt + 1);
    }, STOP_RECOVERY_DELAY_MS);
  };

  const scheduleStopRecovery = (context: StopRecoveryContext) => {
    setTimeout(() => {
      void recoverStoppedConversation(context, 1);
    }, STOP_RECOVERY_DELAY_MS);
  };

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
            streamingPhase: 'thinking',
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
            streamingPhase: null,
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
        await client.stopChat(currentSessionId);
        const recoveryContext = buildStopRecoveryContext(get(), currentSessionId);
        if (recoveryContext) {
          scheduleStopRecovery(recoveryContext);
        }
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
