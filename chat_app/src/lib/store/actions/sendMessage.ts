// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Message } from '../../../types';
import type { SendMessageRuntimeOptions } from '../../../types';
import type ApiClient from '../../api/client';
import type { SessionRuntimeSettingsResponse } from '../../api/client/types';
import {
  getRealtimeConnectionStateSnapshot,
  waitForRealtimeConnectedSnapshot,
} from '../../realtime/state';
import { debugLog, debugLogLazy } from '@/lib/utils';
import {
  assertPayloadWithinTransportBudget,
  prepareAttachmentsForStreaming,
  requestPayloadMaxBytesForAttachmentTotalLimit,
  resolveAttachmentTotalMaxBytes,
} from './sendMessage/attachments';
import { createInternalId } from './sendMessage/internalId';
import { createDraftUserMessage } from './sendMessage/messageFactory';
import {
  buildChatRequestLogPayload,
  buildStreamChatRuntimeOptions,
  resolveEffectivePlanMode,
  resolveModelCapabilities,
} from './sendMessage/requestPayload';
import {
  resolveRuntimeConfig,
  resolveSelectedModelOrThrow,
} from './sendMessage/runtime';
import {
  beginUserTurnInState,
  createDefaultSessionChatState,
  replaceOptimisticUserMessageId,
  setTaskRunnerAsyncUserMessageStatus,
} from './sendMessage/sessionState';
import { normalizePersistedMessage } from './sendMessage/persistedTurnMessages';
import {
  cloneStreamingMessageDraft,
  extractCompactHistoryMessages,
  writeSessionMessagesCache,
} from './sessionsUtils';
import type {
  ChatStoreGet,
  ChatStoreSet,
} from '../types';
import { type StreamingMessage } from './sendMessage/types';
import { rollbackFailedSendMessage } from './sendMessage/streamExecution';

const REALTIME_STREAM_CONNECT_GRACE_MS = 2200;

const mergeMessageByIdAndTime = (messages: Message[] = [], nextMessage: Message): Message[] => {
  const next = [...messages.filter((message) => message.id !== nextMessage.id), nextMessage];
  return next
    .map((message, index) => ({ message, index }))
    .sort((left, right) => {
      const leftTime = left.message.createdAt instanceof Date ? left.message.createdAt.getTime() : 0;
      const rightTime = right.message.createdAt instanceof Date ? right.message.createdAt.getTime() : 0;
      if (Number.isFinite(leftTime) && Number.isFinite(rightTime) && leftTime !== rightTime) {
        return leftTime - rightTime;
      }
      return left.index - right.index;
    })
    .map(({ message }) => message);
};

const loadAttachmentTotalMaxBytes = async (
  client: ApiClient,
  userId: string,
): Promise<number> => {
  try {
    const response = await client.getUserSettings(userId || undefined);
    return resolveAttachmentTotalMaxBytes(
      response?.effective?.ATTACHMENT_TOTAL_MAX_BYTES
        ?? response?.settings?.ATTACHMENT_TOTAL_MAX_BYTES,
    );
  } catch {
    return resolveAttachmentTotalMaxBytes(undefined);
  }
};

const normalizeRuntimeText = (value: unknown): string | null => {
  if (typeof value !== 'string') {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
};

interface SessionRuntimeSnapshot {
  contactAgentId: string | null;
  contactId: string | null;
  remoteConnectionId: string | null;
  selectedModelId: string | null;
  selectedModelName: string | null;
  selectedThinkingLevel: string | null;
  projectId: string | null;
  projectRoot: string | null;
  workspaceRoot: string | null;
  reasoningEnabled: boolean;
  planModeEnabled: boolean;
}

const emptyRuntimeSnapshot = (): SessionRuntimeSnapshot => ({
  contactAgentId: null,
  contactId: null,
  remoteConnectionId: null,
  selectedModelId: null,
  selectedModelName: null,
  selectedThinkingLevel: null,
  projectId: null,
  projectRoot: null,
  workspaceRoot: null,
  reasoningEnabled: false,
  planModeEnabled: false,
});

const runtimeSnapshotFromSettings = (
  settings: SessionRuntimeSettingsResponse,
): SessionRuntimeSnapshot => {
  return {
    ...emptyRuntimeSnapshot(),
    selectedModelId: normalizeRuntimeText(settings.selected_model_id),
    selectedModelName: normalizeRuntimeText(settings.selected_model_name),
    selectedThinkingLevel: normalizeRuntimeText(settings.selected_thinking_level),
    remoteConnectionId: normalizeRuntimeText(settings.remote_connection_id),
    workspaceRoot: normalizeRuntimeText(settings.workspace_root),
    reasoningEnabled: settings.reasoning_enabled === true,
    planModeEnabled: settings.plan_mode_enabled === true,
  };
};

const loadSessionRuntimeSnapshotForSend = async (
  client: ApiClient,
  sessionId: string,
): Promise<SessionRuntimeSnapshot> => {
  const settings = await client.getConversationRuntimeSettings(sessionId);
  return runtimeSnapshotFromSettings(settings);
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
    const tempAssistantId: string | null = null;
    const {
      currentSessionId,
      aiModelConfigs,
      chatConfig,
      sessionChatState,
      activeSystemContext,
    } = get();

    if (!currentSessionId) {
      throw new Error('No active session');
    }

    // 检查是否已经在发送消息，防止重复发送
    const chatState = sessionChatState[currentSessionId] || createDefaultSessionChatState();
    if (chatState.isLoading || chatState.isStreaming || chatState.isStopping) {
      const activeTurnId = typeof chatState.activeTurnId === 'string'
        ? chatState.activeTurnId.trim()
        : '';
      if (!activeTurnId) {
        debugLog('Message sending already in progress but no active turn is available');
        return;
      }

      try {
        const userId = getUserIdParam();
        const attachmentTotalMaxBytes = await loadAttachmentTotalMaxBytes(client, userId);
        const { previewAttachments, apiAttachments } = await prepareAttachmentsForStreaming(
          attachments,
          true,
          {
            dropImagesWhenUnsupported: false,
            maxTotalBytes: attachmentTotalMaxBytes,
          },
        );
        assertPayloadWithinTransportBudget({
          conversation_id: currentSessionId,
          turn_id: activeTurnId,
          content,
          attachments: apiAttachments || [],
        }, requestPayloadMaxBytesForAttachmentTotalLimit(attachmentTotalMaxBytes));

        const guidanceResponse = await client.sendRuntimeGuidance(
          currentSessionId,
          activeTurnId,
          content,
          apiAttachments,
        );
        if (guidanceResponse?.accepted === false) {
          throw new Error('追加指令未被接受');
        }

        const guidanceMessage = normalizePersistedMessage(
          guidanceResponse?.message,
          currentSessionId,
        );
        if (guidanceMessage) {
          const displayGuidanceMessage: Message = previewAttachments.length > 0
            ? {
                ...guidanceMessage,
                metadata: {
                  ...(guidanceMessage.metadata || {}),
                  attachments: previewAttachments.map((attachment) => ({
                    ...attachment,
                    messageId: guidanceMessage.id,
                  })),
                },
              }
            : guidanceMessage;
          set((state) => {
            if (state.currentSessionId === currentSessionId) {
              state.messages = mergeMessageByIdAndTime(state.messages || [], displayGuidanceMessage);
            }

            const cached = state.sessionMessagesCache?.[currentSessionId];
            const cachedMessages = cached?.messages || [];
            const mergedCachedMessages = mergeMessageByIdAndTime(cachedMessages, displayGuidanceMessage);
            writeSessionMessagesCache(state, currentSessionId, {
              messages: cloneStreamingMessageDraft(extractCompactHistoryMessages(mergedCachedMessages)),
              nextBefore: state.sessionMessagePaginationState?.[currentSessionId]?.nextBefore
                ?? cached?.nextBefore
                ?? null,
              loaded: cached?.loaded ?? state.sessionMessagePaginationState?.[currentSessionId]?.loaded ?? true,
            });
          });
        }

        debugLog('✅ 追加指令已提交到当前运行中的轮次');
      } catch (error) {
        const readableError = error instanceof Error ? error.message : '追加指令发送失败';
        console.error('❌ 追加指令发送失败:', readableError, error);
        set((state) => {
          state.error = readableError;
        });
        throw new Error(readableError);
      }
      return;
    }

    const sessionRuntime = await loadSessionRuntimeSnapshotForSend(client, currentSessionId);
    const effectiveSelectedModelId = sessionRuntime.selectedModelId;
    const runtimeOptionsForResolution: SendMessageRuntimeOptions = {
      contactAgentId: typeof runtimeOptions?.contactAgentId === 'string'
        ? runtimeOptions.contactAgentId.trim()
        : null,
      contactId: typeof runtimeOptions?.contactId === 'string'
        ? runtimeOptions.contactId.trim()
        : null,
      projectId: runtimeOptions.projectId,
      projectRoot: runtimeOptions.projectRoot,
    };
    const {
      effectiveContactAgentId,
      effectiveRemoteConnectionId,
      effectiveModelName,
      effectiveThinkingLevel,
      effectiveProjectId,
      effectiveWorkspaceRoot,
      effectiveExecutionRoot,
    } = resolveRuntimeConfig(sessionRuntime, runtimeOptionsForResolution);
    const planMode = resolveEffectivePlanMode({
      projectId: effectiveProjectId,
      planModeEnabled: sessionRuntime.planModeEnabled,
    });
    const selectedModel = resolveSelectedModelOrThrow(
      effectiveSelectedModelId,
      aiModelConfigs,
    );
    const selectedModelForRequest = {
      ...selectedModel,
      model_name: effectiveModelName || selectedModel.model_name,
      thinking_level: effectiveThinkingLevel || selectedModel.thinking_level,
    };
    if (!selectedModelForRequest.model_name?.trim()) {
      throw new Error('Please select a concrete runtime model before sending the message.');
    }

    const conversationTurnId = createInternalId('turn');
    const streamedTextRef = { value: '' };
    const tempAssistantMessage: StreamingMessage = {
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
      } = resolveModelCapabilities(selectedModelForRequest, sessionRuntime.reasoningEnabled);
      const userId = getUserIdParam();
      const attachmentTotalMaxBytes = await loadAttachmentTotalMaxBytes(client, userId);
      const { previewAttachments, apiAttachments } = await prepareAttachmentsForStreaming(
        attachments,
        supportsImages,
        { maxTotalBytes: attachmentTotalMaxBytes },
      );

      // 创建用户消息（仅前端展示，不立即保存数据库）
      const userMessageTime = new Date();
      const userMessage: Message = createDraftUserMessage({
        sessionId: currentSessionId,
        content,
        conversationTurnId,
        selectedModel: selectedModelForRequest,
        previewAttachments,
        createdAt: userMessageTime,
      });
      const turnProcessKey = conversationTurnId || userMessage.id;
      if (userMessage.metadata?.task_runner_async) {
        userMessage.metadata.task_runner_async.source_user_message_id = userMessage.id;
        userMessage.metadata.task_runner_async.source_turn_id = turnProcessKey;
      }

      set((state) => {
        beginUserTurnInState(state, {
          sessionId: currentSessionId,
          userMessage,
          conversationTurnId,
        });
      });

      debugLogLazy(() => ['🚀 开始调用后端流式聊天API:', buildChatRequestLogPayload({
        sessionId: currentSessionId,
        turnId: conversationTurnId,
        content,
        selectedModel: selectedModelForRequest,
        chatConfig,
        systemContext: activeSystemContext?.content || chatConfig.systemPrompt || '',
        attachments: apiAttachments || [],
        reasoningEnabled,
        contactAgentId: effectiveContactAgentId,
        remoteConnectionId: effectiveRemoteConnectionId,
        projectId: effectiveProjectId,
        projectRoot: effectiveExecutionRoot,
        workspaceRoot: effectiveWorkspaceRoot,
        planMode,
      })]);

      const streamRuntimeOptions = buildStreamChatRuntimeOptions({
        turnId: conversationTurnId,
        contactAgentId: effectiveContactAgentId,
        remoteConnectionId: effectiveRemoteConnectionId,
        projectId: effectiveProjectId,
        projectRoot: effectiveExecutionRoot,
        workspaceRoot: effectiveWorkspaceRoot,
        planMode,
      });
      assertPayloadWithinTransportBudget({
        conversation_id: currentSessionId,
        content,
        user_id: userId,
        attachments: apiAttachments || [],
        reasoning_enabled: reasoningEnabled,
        turn_id: streamRuntimeOptions.turnId,
        contact_agent_id: streamRuntimeOptions.contactAgentId || undefined,
        remote_connection_id: Object.prototype.hasOwnProperty.call(
          streamRuntimeOptions,
          'remoteConnectionId',
        )
          ? (streamRuntimeOptions.remoteConnectionId ?? null)
          : undefined,
        project_id: streamRuntimeOptions.projectId || undefined,
        project_root: streamRuntimeOptions.projectRoot || undefined,
        workspace_root: streamRuntimeOptions.workspaceRoot || undefined,
        plan_mode: streamRuntimeOptions.planMode,
        model_config_id: selectedModelForRequest.id,
        ai_model_config: {
          temperature: 0.7,
          model_name: selectedModelForRequest.model_name,
          thinking_level: selectedModelForRequest.thinking_level || null,
        },
      }, requestPayloadMaxBytesForAttachmentTotalLimit(attachmentTotalMaxBytes));
      let preferRealtimeStream = getRealtimeConnectionStateSnapshot() === 'connected';
      if (!preferRealtimeStream) {
        preferRealtimeStream = await waitForRealtimeConnectedSnapshot(REALTIME_STREAM_CONNECT_GRACE_MS);
      }
      if (!preferRealtimeStream) {
        throw new Error('Realtime connection unavailable');
      }

      set((state) => {
        const prev = state.sessionChatState[currentSessionId] || createDefaultSessionChatState();
        state.sessionChatState[currentSessionId] = {
          ...prev,
          streamingTransport: 'realtime',
        };
      });

      const commandResponse = await client.sendChatCommand(
        currentSessionId,
        content,
        selectedModelForRequest,
        userId,
        apiAttachments,
        reasoningEnabled,
        streamRuntimeOptions,
      );
      if (commandResponse?.accepted === false) {
        throw new Error('聊天命令未被接受');
      }
      const persistedUserMessageId = (
        commandResponse?.source_user_message_id
        || commandResponse?.user_message_id
        || null
      );
      let activeUserMessageId = userMessage.id;
      if (persistedUserMessageId) {
        set((state) => {
          activeUserMessageId = replaceOptimisticUserMessageId(
            state,
            userMessage.id,
            persistedUserMessageId,
          );
        });
      }
      set((state) => {
        setTaskRunnerAsyncUserMessageStatus(state, activeUserMessageId, 'processing');
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
