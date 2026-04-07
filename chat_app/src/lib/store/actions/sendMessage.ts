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
import { rollbackFailedSendMessage } from './sendMessage/failureState';
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
  readSessionImConversationId,
  readSessionRuntimeFromMetadata,
} from '../helpers/sessionRuntime';
import { normalizeImConversationMessage } from '../helpers/messageNormalization';
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
  startSessionChat,
}: {
  set: ChatStoreSet;
  get: ChatStoreGet;
  client: ApiClient;
  getUserIdParam: () => string;
  startSessionChat: (
    sessionId: string,
    content: string,
    modelConfig: StreamChatModelConfigPayload,
    userId?: string,
    attachments?: StreamChatAttachmentPayload[],
    reasoningEnabled?: boolean,
    options?: StreamChatOptions,
    pendingContext?: {
      tempAssistantMessage: StreamingMessage;
      tempUserId: string | null;
      conversationTurnId: string;
      streamedTextRef: { value: string };
    }
  ) => Promise<void>;
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
    const effectiveContactId = (
      typeof runtimeOptions?.contactId === 'string'
        ? runtimeOptions.contactId.trim()
        : ''
    ) || (
      typeof sessionRuntime?.contactId === 'string'
        ? sessionRuntime.contactId.trim()
        : ''
    ) || null;
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

    const {
      supportsImages,
      reasoningEnabled,
    } = resolveModelCapabilities(selectedModel, chatConfig);
    const { previewAttachments, apiAttachments } = await prepareAttachmentsForStreaming(
      attachments,
      supportsImages,
    );

    let imConversationId = readSessionImConversationId(runtimeMetadata);

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
      if (!imConversationId && effectiveContactId) {
        const normalizedProjectId = effectiveProjectId || '0';
        const existingConversation = (await client.getImConversations()).find((conversation) => {
          const conversationContactId = typeof conversation?.contact_id === 'string'
            ? conversation.contact_id.trim()
            : '';
          const conversationProjectId = typeof conversation?.project_id === 'string'
            ? conversation.project_id.trim()
            : '';
          return conversationContactId === effectiveContactId
            && (conversationProjectId || '0') === normalizedProjectId;
        });

        const ensuredConversation = existingConversation || await client.createImConversation({
          contact_id: effectiveContactId,
          project_id: normalizedProjectId,
          title: currentSession?.title || null,
        });
        imConversationId = ensuredConversation.id;

        const nextRuntimeMetadata = {
          ...(runtimeMetadata as Record<string, unknown>),
          im: {
            conversation_id: ensuredConversation.id,
            contact_id: ensuredConversation.contact_id,
          },
        };
        set((state) => {
          applySessionRuntimeMetadata(state, currentSessionId, nextRuntimeMetadata);
        });
        void client.updateSession(currentSessionId, { metadata: nextRuntimeMetadata }).catch(() => {});
      }

      if (imConversationId) {
        set((state) => {
          const prev = state.sessionChatState[currentSessionId] || createDefaultSessionChatState();
          state.sessionChatState[currentSessionId] = {
            ...prev,
            isLoading: true,
            isStreaming: false,
            isStopping: false,
            streamingMessageId: null,
            activeTurnId: null,
          };
          if (state.currentSessionId === currentSessionId) {
            state.isLoading = true;
            state.isStreaming = false;
            state.streamingMessageId = null;
          }
        });

        const imMessage = await client.createImConversationMessage(imConversationId, {
          sender_type: 'user',
          message_type: 'text',
          content,
          delivery_status: 'sent',
          client_message_id: conversationTurnId,
          metadata: {
            conversation_turn_id: conversationTurnId,
            legacy_session_id: currentSessionId,
            project_id: effectiveProjectId,
            project_root: effectiveExecutionRoot,
            remote_connection_id: effectiveRemoteConnectionId,
            ...(previewAttachments.length > 0 ? { attachments: previewAttachments } : {}),
            ...(apiAttachments.length > 0 ? { attachments_payload: apiAttachments } : {}),
          },
        });

        set((state) => {
          const normalizedMessage = normalizeImConversationMessage(imMessage, currentSessionId);
          const existingIndex = state.messages.findIndex((message) => message.id === normalizedMessage.id);
          if (existingIndex >= 0) {
            state.messages[existingIndex] = normalizedMessage;
          } else {
            state.messages.push(normalizedMessage);
          }

          const prev = state.sessionChatState[currentSessionId] || createDefaultSessionChatState();
          state.sessionChatState[currentSessionId] = {
            ...prev,
            isLoading: false,
            isStreaming: false,
            isStopping: false,
            streamingMessageId: null,
            activeTurnId: null,
          };
          if (state.currentSessionId === currentSessionId) {
            state.isLoading = false;
            state.isStreaming = false;
            state.streamingMessageId = null;
          }
        });
        return;
      }

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

      debugLog('🚀 开始通过 session websocket 发送聊天请求:', chatRequest);

      await startSessionChat(
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
        {
          tempAssistantMessage,
          tempUserId,
          conversationTurnId,
          streamedTextRef
        }
      );

      debugLog('✅ 聊天请求已通过 websocket 发出，等待服务端事件');
    } catch (error) {
      const readableError = imConversationId
        ? (error instanceof Error ? error.message : '发送消息失败')
        : rollbackFailedSendMessage({
          set,
          currentSessionId,
          tempAssistantId,
          tempAssistantMessage,
          streamedTextRef,
          error,
        });
      if (imConversationId) {
        set((state) => {
          const prev = state.sessionChatState[currentSessionId] || createDefaultSessionChatState();
          state.sessionChatState[currentSessionId] = {
            ...prev,
            isLoading: false,
            isStreaming: false,
            isStopping: false,
            streamingMessageId: null,
            activeTurnId: null,
          };
          if (state.currentSessionId === currentSessionId) {
            state.isLoading = false;
            state.isStreaming = false;
            state.streamingMessageId = null;
            state.error = readableError;
          }
        });
      }
      console.error('❌ 发送消息失败:', readableError, error);

      throw new Error(readableError);
    }
  };
}
