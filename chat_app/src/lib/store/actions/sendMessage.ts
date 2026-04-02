import type { Message } from '../../../types';
import type ApiClient from '../../api/client';
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
  rollbackFailedSendMessage,
  runStreamingAssistantTurn,
} from './sendMessage/streamExecution';
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
  StreamChatAttachmentPayload,
  StreamChatModelConfigPayload,
  StreamChatOptions,
} from '../../api/client/types';
import type {
  ChatStoreGet,
  ChatStoreSet,
  SendMessageRuntimeOptions,
} from '../types';
import { type StreamingMessage } from './sendMessage/types';

// 工厂函数：创建 sendMessage 处理器，注入依赖以便于在 store 外部维护
export function createSendMessageHandler({
  set,
  get,
  client,
  getUserIdParam,
  streamChat,
}: {
  set: ChatStoreSet;
  get: ChatStoreGet;
  client: ApiClient;
  getUserIdParam: () => string;
  streamChat: (
    sessionId: string,
    content: string,
    modelConfig: StreamChatModelConfigPayload,
    userId?: string,
    attachments?: StreamChatAttachmentPayload[],
    reasoningEnabled?: boolean,
    options?: StreamChatOptions,
  ) => Promise<ReadableStream>;
}) {
  return async function sendMessage(
    content: string,
    attachments: File[] = [],
    runtimeOptions: SendMessageRuntimeOptions = {},
  ) {
    let tempUserId: string | null = null;
    let tempAssistantId: string | null = null;
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
      });

      debugLog('🚀 开始调用后端流式聊天API:', chatRequest);

      const response = await streamChat(
        currentSessionId,
        content,
        selectedModel,
        getUserIdParam(),
        apiAttachments,
        reasoningEnabled,
        buildStreamChatRuntimeOptions({
          turnId: conversationTurnId,
          contactAgentId: effectiveContactAgentId,
          remoteConnectionId: effectiveRemoteConnectionId,
          projectId: effectiveProjectId,
          projectRoot: effectiveExecutionRoot,
          mcpEnabled: effectiveMcpEnabled,
          enabledMcpIds: effectiveEnabledMcpIds,
        }),
      );

      if (!response) {
        throw new Error('No response received');
      }

      await runStreamingAssistantTurn({
        set,
        currentSessionId,
        tempAssistantMessage,
        tempUserId,
        conversationTurnId,
        streamedTextRef,
        response,
      });

      debugLog('✅ 消息发送完成');
    } catch (error) {
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
