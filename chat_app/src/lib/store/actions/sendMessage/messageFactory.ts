import type { Message } from '../../../../types';

const buildModelConfigMetadata = (selectedModel: any) => (
  selectedModel
    ? {
        modelConfig: {
          id: selectedModel.id,
          name: selectedModel.name,
          base_url: selectedModel.base_url,
          model_name: selectedModel.model_name,
        },
      }
    : {}
);

export const createDraftUserMessage = ({
  sessionId,
  content,
  conversationTurnId,
  selectedModel,
  previewAttachments,
  createdAt,
}: {
  sessionId: string;
  content: string;
  conversationTurnId: string;
  selectedModel: any;
  previewAttachments: any[];
  createdAt: Date;
}): Message => ({
  id: `temp_user_${Date.now()}_${Math.random().toString(36).slice(2, 9)}`,
  sessionId,
  role: 'user',
  content,
  status: 'completed',
  createdAt,
  metadata: {
    conversation_turn_id: conversationTurnId,
    ...(previewAttachments.length > 0 ? { attachments: previewAttachments as any } : {}),
    model: selectedModel.model_name,
    ...buildModelConfigMetadata(selectedModel),
    historyProcess: {
      hasProcess: false,
      toolCallCount: 0,
      thinkingCount: 0,
      processMessageCount: 0,
      userMessageId: '',
      finalAssistantMessageId: null,
      expanded: false,
      loaded: false,
      loading: false,
    },
  },
});

export const createDraftAssistantMessage = ({
  sessionId,
  conversationTurnId,
  selectedModel,
  userMessage,
  userMessageTime,
}: {
  sessionId: string;
  conversationTurnId: string;
  selectedModel: any;
  userMessage: Message;
  userMessageTime: Date;
}) => ({
  id: `temp_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
  sessionId,
  role: 'assistant' as const,
  content: '',
  status: 'streaming' as const,
  createdAt: new Date(userMessageTime.getTime() + 1),
  metadata: {
    conversation_turn_id: conversationTurnId,
    model: selectedModel.model_name,
    ...buildModelConfigMetadata(selectedModel),
    historyFinalForUserMessageId: userMessage.id,
    historyFinalForTurnId: conversationTurnId,
    historyProcessExpanded: false,
    historyDraftUserMessage: {
      id: userMessage.id,
      content: userMessage.content,
      createdAt: userMessageTime.toISOString(),
    },
    toolCalls: [],
    contentSegments: [{ content: '', type: 'text' as const }],
    currentSegmentIndex: 0,
  },
});
