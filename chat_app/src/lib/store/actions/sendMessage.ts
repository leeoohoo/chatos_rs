import type { Message } from '../../../types';
import type { SendMessageRuntimeOptions } from '../../../types';
import type ApiClient from '../../api/client';
import { ApiRequestError } from '../../api/client/shared';
import {
  getRealtimeConnectionStateSnapshot,
  waitForRealtimeConnectedSnapshot,
} from '../../realtime/state';
import { debugLog } from '@/lib/utils';
import { prepareAttachmentsForStreaming } from './sendMessage/attachments';
import { createInternalId } from './sendMessage/internalId';
import {
  createDraftAssistantMessage,
  createDraftUserMessage,
} from './sendMessage/messageFactory';
import {
  buildChatRequestLogPayload,
  buildStreamChatRuntimeOptions,
  resolveModelCapabilities,
} from './sendMessage/requestPayload';
import {
  resolveRuntimeConfig,
  resolveSelectedModelOrThrow,
} from './sendMessage/runtime';
import {
  applySessionRuntimeMetadata,
  beginAssistantDraftInState,
  beginUserTurnInState,
  createDefaultSessionChatState,
} from './sendMessage/sessionState';
import {
  mergeSessionRuntimeIntoMetadata,
  readSessionRuntimeFromMetadata,
} from '../helpers/sessionRuntime';
import type {
  ChatStoreGet,
  ChatStoreSet,
} from '../types';
import { type StreamingMessage } from './sendMessage/types';

const REALTIME_STREAM_CONNECT_GRACE_MS = 2200;

const shouldFallbackToSseForRealtimeCommandError = (error: unknown): boolean => {
  if (error instanceof ApiRequestError) {
    if (error.status >= 400 && error.status < 500) {
      return false;
    }
    return true;
  }
  return true;
};

// 工厂函数：创建 sendMessage 处理器，注入依赖以便于在 store 外部维护
export function createSendMessageHandler({
  set,
  get,
  client,
  getUserIdParam,
}: {
  set: ChatStoreSet;
  get: ChatStoreGet;
  client: ApiClient;
  getUserIdParam: () => string;
}) {
  return async function sendMessage(
    content: string,
    attachments: File[] = [],
    runtimeOptions: SendMessageRuntimeOptions = {},
  ) {
    let tempUserId: string | null = null;
    let tempAssistantId: string | null = null;
    let streamExecutionModulePromise:
      | Promise<typeof import('./sendMessage/streamExecution')>
      | null = null;
    const {
      currentSessionId,
      currentSession,
      selectedModelId,
      selectedAgentId,
      aiModelConfigs,
      chatConfig,
      sessionChatState,
      activeSystemContext,
      sessionAiSelectionBySession,
    } = get();

    if (!currentSessionId) {
      throw new Error('No active session');
    }

    // 检查是否已经在发送消息，防止重复发送
    const chatState = sessionChatState[currentSessionId] || createDefaultSessionChatState();
    if (chatState.isLoading || chatState.isStreaming || chatState.isStopping) {
      debugLog('Message sending already in progress, ignoring duplicate request');
      return;
    }

    const sessionAiSelection = sessionAiSelectionBySession?.[currentSessionId];
    const effectiveSelectedModelId = sessionAiSelection?.selectedModelId ?? selectedModelId;
    const sessionRuntime = readSessionRuntimeFromMetadata(currentSession?.metadata);
    const fallbackContactAgentId = (
      typeof runtimeOptions?.contactAgentId === 'string'
        ? runtimeOptions.contactAgentId.trim()
        : ''
    ) || (
      typeof sessionAiSelection?.selectedAgentId === 'string'
        ? sessionAiSelection.selectedAgentId.trim()
        : ''
    ) || (
      typeof selectedAgentId === 'string'
        ? selectedAgentId.trim()
        : ''
    ) || null;
    const runtimeOptionsWithContactFallback: SendMessageRuntimeOptions = {
      ...runtimeOptions,
      contactAgentId: fallbackContactAgentId,
    };
    const {
      effectiveContactAgentId,
      effectiveRemoteConnectionId,
      effectiveProjectId,
      effectiveProjectRoot,
      effectiveWorkspaceRoot,
      effectiveExecutionRoot,
      effectiveMcpEnabled,
      effectiveEnabledMcpIds,
    } = resolveRuntimeConfig(sessionRuntime, runtimeOptionsWithContactFallback);
    const effectiveSkillsEnabled = runtimeOptionsWithContactFallback.skillsEnabled === true;
    const effectiveSelectedSkillIds = Array.isArray(runtimeOptionsWithContactFallback.selectedSkillIds)
      ? runtimeOptionsWithContactFallback.selectedSkillIds
        .map((item: string) => (typeof item === 'string' ? item.trim() : ''))
        .filter((item: string, index: number, arr: string[]) => item.length > 0 && arr.indexOf(item) === index)
      : [];
    const selectedModel = resolveSelectedModelOrThrow(
      effectiveSelectedModelId,
      aiModelConfigs,
    );

    const runtimeMetadata = mergeSessionRuntimeIntoMetadata(currentSession?.metadata, {
      selectedModelId: selectedModel?.id || null,
      contactAgentId: effectiveContactAgentId,
      remoteConnectionId: effectiveRemoteConnectionId,
      projectId: effectiveProjectId,
      projectRoot: effectiveProjectRoot,
      workspaceRoot: effectiveWorkspaceRoot,
      mcpEnabled: effectiveMcpEnabled,
      enabledMcpIds: effectiveEnabledMcpIds,
    });
    set((state) => {
      applySessionRuntimeMetadata(state, currentSessionId, runtimeMetadata);
    });
    void client.updateSession(currentSessionId, { metadata: runtimeMetadata }).catch(() => {});

    const conversationTurnId = createInternalId('turn');
    const streamedTextRef = { value: '' };
    let tempAssistantMessage: StreamingMessage = {
      id: '',
      sessionId: currentSessionId,
      role: 'assistant' as const,
      content: '',
      status: 'streaming' as const,
      createdAt: new Date(),
      metadata: {},
    };
    try {
      const {
        supportsImages,
        reasoningEnabled,
      } = resolveModelCapabilities(selectedModel, chatConfig);
      const { previewAttachments, apiAttachments } = await prepareAttachmentsForStreaming(
        attachments,
        supportsImages,
      );

      // 创建用户消息（仅前端展示，不立即保存数据库）
      const userMessageTime = new Date();
      const userMessage: Message = createDraftUserMessage({
        sessionId: currentSessionId,
        content,
        conversationTurnId,
        selectedModel,
        previewAttachments,
        createdAt: userMessageTime,
      });
      tempUserId = userMessage.id;
      const turnProcessKey = conversationTurnId || userMessage.id;
      if (userMessage.metadata?.historyProcess) {
        userMessage.metadata.historyProcess.userMessageId = userMessage.id;
        userMessage.metadata.historyProcess.turnId = turnProcessKey;
      }

      set((state) => {
        beginUserTurnInState(state, {
          sessionId: currentSessionId,
          userMessage,
          turnProcessKey,
          conversationTurnId,
        });
      });

      // 创建临时的助手消息用于UI显示，但不保存到数据库
      tempAssistantMessage = createDraftAssistantMessage({
        sessionId: currentSessionId,
        conversationTurnId,
        selectedModel,
        userMessage,
        userMessageTime,
      });
      tempAssistantId = tempAssistantMessage.id;

      set((state) => {
        beginAssistantDraftInState(state, {
          sessionId: currentSessionId,
          userMessageId: userMessage.id,
          assistantMessage: tempAssistantMessage,
          conversationTurnId,
        });
      });

      const chatRequest = buildChatRequestLogPayload({
        sessionId: currentSessionId,
        turnId: conversationTurnId,
        content,
        selectedModel,
        chatConfig,
        systemContext: activeSystemContext?.content || chatConfig.systemPrompt || '',
        attachments: apiAttachments || [],
        reasoningEnabled,
        contactAgentId: effectiveContactAgentId,
        remoteConnectionId: effectiveRemoteConnectionId,
        projectId: effectiveProjectId,
        projectRoot: effectiveExecutionRoot,
        mcpEnabled: effectiveMcpEnabled,
        enabledMcpIds: effectiveEnabledMcpIds,
        skillsEnabled: effectiveSkillsEnabled,
        selectedSkillIds: effectiveSelectedSkillIds,
      });

      debugLog('🚀 开始调用后端流式聊天API:', chatRequest);

      const streamRuntimeOptions = buildStreamChatRuntimeOptions({
        turnId: conversationTurnId,
        contactAgentId: effectiveContactAgentId,
        remoteConnectionId: effectiveRemoteConnectionId,
        projectId: effectiveProjectId,
        projectRoot: effectiveExecutionRoot,
        mcpEnabled: effectiveMcpEnabled,
        enabledMcpIds: effectiveEnabledMcpIds,
        skillsEnabled: effectiveSkillsEnabled,
        selectedSkillIds: effectiveSelectedSkillIds,
      });
      let preferRealtimeStream = getRealtimeConnectionStateSnapshot() === 'connected';
      if (!preferRealtimeStream) {
        preferRealtimeStream = await waitForRealtimeConnectedSnapshot(REALTIME_STREAM_CONNECT_GRACE_MS);
      }

      let shouldFallbackToSse = false;
      let realtimeCommandError: unknown = null;
      if (preferRealtimeStream) {
        set((state) => {
          const prev = state.sessionChatState[currentSessionId] || createDefaultSessionChatState();
          state.sessionChatState[currentSessionId] = {
            ...prev,
            streamingTransport: 'realtime',
          };
        });
        try {
          const commandResponse = await client.sendChatCommand(
            currentSessionId,
            content,
            selectedModel,
            getUserIdParam(),
            apiAttachments,
            reasoningEnabled,
            streamRuntimeOptions,
          );
          if (commandResponse?.accepted === false) {
            throw new Error('聊天命令未被接受');
          }
        } catch (error) {
          realtimeCommandError = error;
          shouldFallbackToSse = shouldFallbackToSseForRealtimeCommandError(error);
          if (shouldFallbackToSse) {
            debugLog('⚠️ realtime send command failed, fallback to SSE', {
              error: error instanceof Error ? error.message : String(error),
              conversationTurnId,
              currentSessionId,
            });
          } else {
            debugLog('⛔ realtime send command rejected, skip SSE fallback', {
              error: error instanceof Error ? error.message : String(error),
              errorStatus: error instanceof ApiRequestError ? error.status : null,
              conversationTurnId,
              currentSessionId,
            });
            throw error;
          }
        }
      } else {
        shouldFallbackToSse = true;
      }

      if (shouldFallbackToSse) {
        set((state) => {
          const prev = state.sessionChatState[currentSessionId] || createDefaultSessionChatState();
          state.sessionChatState[currentSessionId] = {
            ...prev,
            streamingTransport: 'sse',
          };
        });
        const response = await client.streamChat(
          currentSessionId,
          content,
          selectedModel,
          getUserIdParam(),
          apiAttachments,
          reasoningEnabled,
          streamRuntimeOptions,
        );

        if (!response) {
          throw new Error('No response received');
        }

        streamExecutionModulePromise ??= import('./sendMessage/streamExecution');
        const { runStreamingAssistantTurn } = await streamExecutionModulePromise;
        await runStreamingAssistantTurn({
          apiClient: client,
          set,
          getCurrentState: () => {
            const state = get();
            return {
              currentSessionId: state.currentSessionId,
              messages: state.messages,
              loadMessages: state.loadMessages,
            };
          },
          currentSessionId,
          tempAssistantMessage,
          tempUserId,
          conversationTurnId,
          streamedTextRef,
          response,
        });
      }

      if (realtimeCommandError) {
        debugLog('ℹ️ SSE fallback completed after realtime command failure', {
          conversationTurnId,
          currentSessionId,
        });
      }

      debugLog('✅ 消息发送完成');
    } catch (error) {
      streamExecutionModulePromise ??= import('./sendMessage/streamExecution');
      const { rollbackFailedSendMessage } = await streamExecutionModulePromise;
      const readableError = rollbackFailedSendMessage({
        set,
        currentSessionId,
        tempAssistantId,
        tempAssistantMessage,
        streamedTextRef,
        error,
      });
      console.error('❌ 发送消息失败:', readableError, error);

      throw new Error(readableError);
    }
  };
}
