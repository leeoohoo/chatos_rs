import type { Message } from '../../../types';
import type { SendMessageRuntimeOptions } from '../../../types';
import type ApiClient from '../../api/client';
import {
  getRealtimeConnectionStateSnapshot,
  waitForRealtimeConnectedSnapshot,
} from '../../realtime/state';
import { debugLog } from '@/lib/utils';
import {
  assertPayloadWithinTransportBudget,
  prepareAttachmentsForStreaming,
} from './sendMessage/attachments';
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
import { rollbackFailedSendMessage } from './sendMessage/streamExecution';

const REALTIME_STREAM_CONNECT_GRACE_MS = 2200;

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
    const requestedModelConfigId = typeof runtimeOptions?.modelConfigId === 'string'
      ? runtimeOptions.modelConfigId.trim()
      : '';
    const effectiveSelectedModelId = requestedModelConfigId || sessionAiSelection?.selectedModelId || selectedModelId;
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
      effectiveModelName,
      effectiveThinkingLevel,
      effectiveProjectId,
      effectiveProjectRoot,
      effectiveWorkspaceRoot,
      effectiveExecutionRoot,
      effectiveMcpEnabled,
      effectiveEnabledMcpIds,
      effectiveAutoCreateTask,
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
    const selectedModelForRequest = {
      ...selectedModel,
      model_name: effectiveModelName || selectedModel.model_name,
      thinking_level: effectiveThinkingLevel || selectedModel.thinking_level,
    };

    const runtimeMetadata = mergeSessionRuntimeIntoMetadata(currentSession?.metadata, {
      selectedModelId: selectedModel?.id || null,
      selectedModelName: selectedModelForRequest.model_name || null,
      selectedThinkingLevel: selectedModelForRequest.thinking_level || null,
      contactAgentId: effectiveContactAgentId,
      remoteConnectionId: effectiveRemoteConnectionId,
      projectId: effectiveProjectId,
      projectRoot: effectiveProjectRoot,
      workspaceRoot: effectiveWorkspaceRoot,
      mcpEnabled: effectiveMcpEnabled,
      enabledMcpIds: effectiveEnabledMcpIds,
      autoCreateTask: effectiveAutoCreateTask,
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
      } = resolveModelCapabilities(selectedModelForRequest, chatConfig);
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
        selectedModel: selectedModelForRequest,
        previewAttachments,
        createdAt: userMessageTime,
        taskRunnerAsyncContactMode: runtimeOptionsWithContactFallback.taskRunnerAsyncContactMode === true,
      });
      const turnProcessKey = conversationTurnId || userMessage.id;
      if (userMessage.metadata?.historyProcess) {
        userMessage.metadata.historyProcess.userMessageId = userMessage.id;
        userMessage.metadata.historyProcess.turnId = turnProcessKey;
      }
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

      // 创建临时的助手消息用于UI显示，但不保存到数据库
      tempAssistantMessage = createDraftAssistantMessage({
        sessionId: currentSessionId,
        conversationTurnId,
        selectedModel: selectedModelForRequest,
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
        selectedModel: selectedModelForRequest,
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
        autoCreateTask: effectiveAutoCreateTask,
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
        autoCreateTask: effectiveAutoCreateTask,
        skillsEnabled: effectiveSkillsEnabled,
        selectedSkillIds: effectiveSelectedSkillIds,
      });
      const userId = getUserIdParam();
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
        mcp_enabled: streamRuntimeOptions.mcpEnabled,
        enabled_mcp_ids: streamRuntimeOptions.enabledMcpIds || [],
        auto_create_task: streamRuntimeOptions.autoCreateTask,
        skills_enabled: streamRuntimeOptions.skillsEnabled === true,
        selected_skill_ids: streamRuntimeOptions.selectedSkillIds || [],
        model_config_id: selectedModelForRequest.id,
        ai_model_config: {
          temperature: 0.7,
          model_name: selectedModelForRequest.model_name,
          thinking_level: selectedModelForRequest.thinking_level || null,
        },
      });
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
