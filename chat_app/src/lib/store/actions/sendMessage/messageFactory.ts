import type { AiModelConfig, Message } from '../../../../types';
import {
  buildModelConfigMetadata,
  createDefaultHistoryProcessState,
  type PreviewAttachment,
  type StreamingMessage,
} from './types';

export const createDraftUserMessage = ({
  sessionId,
  content,
  conversationTurnId,
  selectedModel,
  previewAttachments,
  createdAt,
  taskRunnerAsyncContactMode = false,
}: {
  sessionId: string;
  content: string;
  conversationTurnId: string;
  selectedModel: AiModelConfig;
  previewAttachments: PreviewAttachment[];
  createdAt: Date;
  taskRunnerAsyncContactMode?: boolean;
}): Message => ({
  id: `temp_user_${Date.now()}_${Math.random().toString(36).slice(2, 9)}`,
  sessionId,
  role: 'user',
  content,
  status: 'completed',
  createdAt,
  metadata: {
    conversation_turn_id: conversationTurnId,
    ...(previewAttachments.length > 0 ? { attachments: previewAttachments } : {}),
    model: selectedModel.model_name,
    ...buildModelConfigMetadata(selectedModel),
    historyProcess: createDefaultHistoryProcessState({
      userMessageId: '',
      turnId: conversationTurnId,
      finalAssistantMessageId: null,
    }),
    ...(taskRunnerAsyncContactMode
      ? {
        task_runner_async: {
          mode: 'contact_async',
          overall_status: 'pending',
          source_turn_id: conversationTurnId,
        },
      }
      : {}),
  },
});

export const createDraftAssistantMessage = ({
  sessionId,
  conversationTurnId,
  selectedModel,
  userMessage,
  userMessageTime,
  taskRunnerAsyncContactMode = false,
}: {
  sessionId: string;
  conversationTurnId: string;
  selectedModel: AiModelConfig;
  userMessage: Message;
  userMessageTime: Date;
  taskRunnerAsyncContactMode?: boolean;
}): StreamingMessage => ({
  id: `temp_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
  sessionId,
  role: 'assistant' as const,
  content: '',
  messageMode: taskRunnerAsyncContactMode ? 'task_runner_async_plan' : undefined,
  status: 'streaming' as const,
  createdAt: new Date(userMessageTime.getTime() + 1),
  metadata: {
    conversation_turn_id: conversationTurnId,
    model: selectedModel.model_name,
    ...buildModelConfigMetadata(selectedModel),
    historyFinalForUserMessageId: userMessage.id,
    historyFinalForTurnId: conversationTurnId,
    historyDraftUserMessage: {
      id: userMessage.id,
      content: userMessage.content,
      createdAt: userMessageTime.toISOString(),
    },
    ...(taskRunnerAsyncContactMode
      ? {
        task_runner_async: {
          mode: 'contact_async',
          message_kind: 'plan_summary',
          source_turn_id: conversationTurnId,
          source_user_message_id: userMessage.id,
        },
      }
      : {}),
    toolCalls: [],
    contentSegments: [{ content: '', type: 'text' as const }],
    currentSegmentIndex: 0,
  },
});
